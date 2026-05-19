//! Render-trace harness.
//!
//! A feature-gated, per-process JSONL recorder that captures four event
//! kinds:
//!
//! - `KeypressDispatched` — a key reached the FM input handler.
//! - `FramePainted` — the FM render closure returned (provisional;
//!   accurate prepaint/paint/draw split arrives with
//!   `TASK:phase-17/fm-render-custom-row`).
//! - `PreviewUpgraded` — the FM upgraded the preview column from the
//!   lightweight snapshot to the real `EditorElement`.
//! - `SwitchTiming` — codon-session emits one per window-/session-switch
//!   boundary with per-phase durations (capture / restore / persist) so
//!   `scripts/render-trace-report.py --kind switch` can compute the
//!   phase-17 switch-budget percentiles.
//!
//! The harness has two enable paths:
//!
//! - The CLI flag `--render-trace[=FILE]` (parsed in `apps/codon`).
//! - The settings field `[diagnostics] render_trace = true` plus an
//!   optional `[diagnostics] render_trace_path = "..."` in
//!   `~/.config/codon/codon.toml` (loaded via `codon-config`).
//!
//! The CLI takes precedence. The default output path is
//! `$XDG_STATE_HOME/codon/render-trace/codon-<unix-ts>.jsonl` (with
//! fallback to `$HOME/.local/state/codon/render-trace/...`).
//!
//! Storage is a `Mutex<Vec<TraceEvent>>`. Events are pushed inline; the
//! whole buffer is flushed to disk in `Drop`. The hot path avoids
//! allocations (`SharedString` clones are `Arc` bumps; `PathBuf` only
//! appears in `PreviewUpgraded`, fired at most once per dwell).
//!
//! Per-event overhead target is ~50 ns — a `Mutex` lock + `Vec::push`.
//! That fits inside the FM's frame budget (≤ 5 ms p95) with thousands
//! of events per frame to spare; the trace itself can't push the FM
//! over budget at navigation rates.

use std::{
    fs::{self, File},
    io::{BufWriter, Write as _},
    path::{Path, PathBuf},
    sync::{atomic::{AtomicU64, Ordering}, Mutex, OnceLock},
    time::Instant,
};

use gpui::SharedString;

/// Switch boundary classification carried by [`TraceEvent::SwitchTiming`].
/// `Window` is an intra-session window cycle (`prefix Tab` / `prefix N` /
/// `prefix P`); `Session` is a `codon_session::SessionSwitch` /
/// `attach_session` boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchKind {
    Window,
    Session,
}

impl SwitchKind {
    fn as_str(self) -> &'static str {
        match self {
            SwitchKind::Window => "window",
            SwitchKind::Session => "session",
        }
    }
}

/// Outcome of the [`WindowRuntimeCache`] lookup for the incoming window.
///
/// `Hit` — runtime cache served the restore (cheapest path);
/// `Miss` — cache absent or expired, fell back to the persisted
/// `LayoutSnapshot` (`swap::apply` path);
/// `Cold` — neither cache nor snapshot was available, a fresh empty
/// pane was installed instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOutcome {
    Hit,
    Miss,
    Cold,
}

impl CacheOutcome {
    fn as_str(self) -> &'static str {
        match self {
            CacheOutcome::Hit => "hit",
            CacheOutcome::Miss => "miss",
            CacheOutcome::Cold => "cold",
        }
    }
}

/// One traced event. Variants stay POD-cheap so `record` never allocates
/// on the hot path beyond a single `Vec::push`.
#[derive(Debug)]
pub(crate) enum TraceEvent {
    KeypressDispatched {
        at: Instant,
        key: SharedString,
    },
    FramePainted {
        at: Instant,
        prepaint_ms: f32,
        paint_ms: f32,
        draw_ms: f32,
        rows_painted: u32,
        cache_hits: u32,
        cache_misses: u32,
    },
    PreviewUpgraded {
        at: Instant,
        path: PathBuf,
    },
    SwitchTiming {
        at: Instant,
        kind: SwitchKind,
        /// Codon `WindowId` of the outgoing window (raw `u64`). `None`
        /// for the first switch of a freshly-launched process when no
        /// window was previously attached.
        outgoing: Option<u64>,
        /// Codon `WindowId` of the incoming window (raw `u64`). Same
        /// `None` semantics as `outgoing` (e.g. switch to a brand-new
        /// session with no windows yet).
        incoming: Option<u64>,
        /// Time spent inside `swap::capture` for the outgoing window.
        /// `0.0` when the cache-hit fast path elided the capture
        /// (`c-skip-capture-on-cache-hit`).
        capture_ms: f32,
        /// Time spent inside `capture_runtime` building the in-memory
        /// `WindowRuntime` Arc bumps. Distinguished from `capture_ms`
        /// because the fast path keeps this even when it drops the
        /// snapshot capture.
        runtime_capture_ms: f32,
        /// Time spent inside `Workspace::restore_center_root` (or the
        /// fallback `swap::apply` path on cache miss).
        restore_ms: f32,
        /// Time spent enqueuing the persist task (`mark_dirty` /
        /// `flush_now`). The actual JSON serialization runs on the
        /// background executor and is not included here.
        persist_scheduled_ms: f32,
        cache_outcome: CacheOutcome,
    },
}

/// The collector. One instance per process, lazily installed by
/// [`install`]. Subsequent `install` calls are a no-op so the CLI and
/// settings paths can both invoke it safely.
pub(crate) struct RenderTrace {
    events: Mutex<Vec<TraceEvent>>,
    path: PathBuf,
    /// Process start `Instant` — every event's `at_ms` is relative to
    /// this so JSONL stays self-contained without needing absolute
    /// timestamps.
    origin: Instant,
}

impl RenderTrace {
    fn new(path: PathBuf) -> Self {
        Self {
            events: Mutex::new(Vec::with_capacity(4096)),
            path,
            origin: Instant::now(),
        }
    }

    /// Push an event onto the buffer. Inlined hot path: a single lock
    /// + push. No allocation beyond the (cheap) `Vec` growth amortised
    /// over `with_capacity`.
    #[inline]
    pub(crate) fn record(&self, event: TraceEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
        // Poisoned mutex: drop the event silently. A panicked recorder
        // shouldn't compound the original failure by panicking again.
    }

    fn flush(&self) {
        let events = match self.events.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => return,
        };
        if let Some(parent) = self.path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            eprintln!(
                "codon: render-trace: failed to create {}: {err}",
                parent.display()
            );
            return;
        }
        let file = match File::create(&self.path) {
            Ok(f) => f,
            Err(err) => {
                eprintln!(
                    "codon: render-trace: failed to open {}: {err}",
                    self.path.display()
                );
                return;
            }
        };
        let mut out = BufWriter::new(file);
        for event in &events {
            let at_ms = at_ms(event, self.origin);
            let line = match event {
                TraceEvent::KeypressDispatched { key, .. } => format!(
                    "{{\"t\":\"keypress\",\"at_ms\":{at_ms:.3},\"key\":{}}}",
                    json_string(key.as_ref()),
                ),
                TraceEvent::FramePainted {
                    prepaint_ms,
                    paint_ms,
                    draw_ms,
                    rows_painted,
                    cache_hits,
                    cache_misses,
                    ..
                } => format!(
                    "{{\"t\":\"frame_painted\",\"at_ms\":{at_ms:.3},\
                     \"prepaint_ms\":{prepaint_ms:.3},\"paint_ms\":{paint_ms:.3},\
                     \"draw_ms\":{draw_ms:.3},\"rows_painted\":{rows_painted},\
                     \"cache_hits\":{cache_hits},\"cache_misses\":{cache_misses}}}"
                ),
                TraceEvent::PreviewUpgraded { path, .. } => format!(
                    "{{\"t\":\"preview_upgraded\",\"at_ms\":{at_ms:.3},\"path\":{}}}",
                    json_string(&path.display().to_string()),
                ),
                TraceEvent::SwitchTiming {
                    kind,
                    outgoing,
                    incoming,
                    capture_ms,
                    runtime_capture_ms,
                    restore_ms,
                    persist_scheduled_ms,
                    cache_outcome,
                    ..
                } => format!(
                    "{{\"t\":\"switch\",\"at_ms\":{at_ms:.3},\"kind\":\"{}\",\
                     \"outgoing\":{},\"incoming\":{},\
                     \"capture_ms\":{capture_ms:.3},\
                     \"runtime_capture_ms\":{runtime_capture_ms:.3},\
                     \"restore_ms\":{restore_ms:.3},\
                     \"persist_scheduled_ms\":{persist_scheduled_ms:.3},\
                     \"cache_outcome\":\"{}\"}}",
                    kind.as_str(),
                    outgoing
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "null".to_string()),
                    incoming
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "null".to_string()),
                    cache_outcome.as_str(),
                ),
            };
            if writeln!(out, "{line}").is_err() {
                break;
            }
        }
        if let Err(err) = out.flush() {
            eprintln!(
                "codon: render-trace: failed to flush {}: {err}",
                self.path.display()
            );
        } else {
            log::info!(
                "codon: render-trace: wrote {} events to {}",
                events.len(),
                self.path.display()
            );
        }
    }
}

impl Drop for RenderTrace {
    fn drop(&mut self) {
        self.flush();
    }
}

fn at_ms(event: &TraceEvent, origin: Instant) -> f64 {
    let at = match event {
        TraceEvent::KeypressDispatched { at, .. }
        | TraceEvent::FramePainted { at, .. }
        | TraceEvent::PreviewUpgraded { at, .. }
        | TraceEvent::SwitchTiming { at, .. } => *at,
    };
    at.saturating_duration_since(origin).as_secs_f64() * 1000.0
}

/// Minimal JSON string escaper. Avoids pulling `serde_json` into the
/// hot flush path; the file is JSONL with at most a key string + a
/// path string per event.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

static GLOBAL: OnceLock<RenderTrace> = OnceLock::new();

/// Install the global trace collector at the given path. Idempotent —
/// the first call wins so the CLI flag (installed before `Application::run`)
/// takes precedence over the settings field (applied during init).
pub(crate) fn install(path: PathBuf) {
    let _ = GLOBAL.set(RenderTrace::new(path));
}

/// Public CLI/settings entry point. Wrapped so callers don't have to
/// import the module internals.
pub fn install_global(path: PathBuf) {
    install(path);
}

/// Resolve the default output path when no explicit path is supplied.
/// Lives in `$XDG_STATE_HOME/codon/render-trace/` (or the
/// `$HOME/.local/state/codon/render-trace/` fallback) with a
/// per-session filename so repeated launches don't clobber each
/// other.
pub fn default_trace_path() -> PathBuf {
    let dir = if let Some(xdg) = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
    {
        xdg.join("codon").join("render-trace")
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        home.join(".local")
            .join("state")
            .join("codon")
            .join("render-trace")
    } else {
        PathBuf::from(".").join("codon-render-trace")
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    dir.join(format!("codon-{stamp}.jsonl"))
}

/// Convenience: record a [`TraceEvent::KeypressDispatched`]. No-op when
/// the collector is not installed. The check is a single relaxed
/// `OnceLock::get` — cheap enough to leave in the hot path
/// unconditionally.
#[inline]
pub(crate) fn record_keypress(key: SharedString) {
    if let Some(trace) = GLOBAL.get() {
        trace.record(TraceEvent::KeypressDispatched {
            at: Instant::now(),
            key,
        });
    }
}

/// Convenience: record a [`TraceEvent::FramePainted`].
#[inline]
pub(crate) fn record_frame(
    prepaint_ms: f32,
    paint_ms: f32,
    draw_ms: f32,
    rows_painted: u32,
    cache_hits: u32,
    cache_misses: u32,
) {
    if let Some(trace) = GLOBAL.get() {
        trace.record(TraceEvent::FramePainted {
            at: Instant::now(),
            prepaint_ms,
            paint_ms,
            draw_ms,
            rows_painted,
            cache_hits,
            cache_misses,
        });
    }
}

/// Convenience: record a [`TraceEvent::PreviewUpgraded`].
#[inline]
pub(crate) fn record_preview_upgraded(path: &Path) {
    if let Some(trace) = GLOBAL.get() {
        trace.record(TraceEvent::PreviewUpgraded {
            at: Instant::now(),
            path: path.to_path_buf(),
        });
    }
}

/// Record a [`TraceEvent::SwitchTiming`]. Called from `codon-session`
/// (and indirectly from vendored Zed's `restore_center_root` via the
/// `set_restore_timing_callback` shim) at the boundary of each window-
/// or session-switch. Returns immediately when the trace collector is
/// not installed.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn record_switch(
    kind: SwitchKind,
    outgoing: Option<u64>,
    incoming: Option<u64>,
    capture_ms: f32,
    runtime_capture_ms: f32,
    restore_ms: f32,
    persist_scheduled_ms: f32,
    cache_outcome: CacheOutcome,
) {
    if let Some(trace) = GLOBAL.get() {
        trace.record(TraceEvent::SwitchTiming {
            at: Instant::now(),
            kind,
            outgoing,
            incoming,
            capture_ms,
            runtime_capture_ms,
            restore_ms,
            persist_scheduled_ms,
            cache_outcome,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: install, record one of each variant, flush via
    /// `Drop`, read the file back. Uses a fresh `RenderTrace` (not the
    /// global) so multiple tests don't fight over the `OnceLock`.
    #[test]
    fn flushes_jsonl_on_drop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("trace.jsonl");
        {
            let trace = RenderTrace::new(path.clone());
            trace.record(TraceEvent::KeypressDispatched {
                at: Instant::now(),
                key: SharedString::from("j"),
            });
            trace.record(TraceEvent::FramePainted {
                at: Instant::now(),
                prepaint_ms: 0.4,
                paint_ms: 2.1,
                draw_ms: 1.6,
                rows_painted: 30,
                cache_hits: 28,
                cache_misses: 2,
            });
            trace.record(TraceEvent::PreviewUpgraded {
                at: Instant::now(),
                path: PathBuf::from("/tmp/foo.rs"),
            });
            trace.record(TraceEvent::SwitchTiming {
                at: Instant::now(),
                kind: SwitchKind::Window,
                outgoing: Some(3),
                incoming: Some(4),
                capture_ms: 0.0,
                runtime_capture_ms: 0.1,
                restore_ms: 2.3,
                persist_scheduled_ms: 0.05,
                cache_outcome: CacheOutcome::Hit,
            });
        }
        let content = std::fs::read_to_string(&path).expect("read trace");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 4, "one line per event");
        assert!(lines[0].contains("\"t\":\"keypress\""));
        assert!(lines[0].contains("\"key\":\"j\""));
        assert!(lines[1].contains("\"t\":\"frame_painted\""));
        assert!(lines[1].contains("\"rows_painted\":30"));
        assert!(lines[2].contains("\"t\":\"preview_upgraded\""));
        assert!(lines[2].contains("/tmp/foo.rs"));
        assert!(lines[3].contains("\"t\":\"switch\""));
        assert!(lines[3].contains("\"kind\":\"window\""));
        assert!(lines[3].contains("\"outgoing\":3"));
        assert!(lines[3].contains("\"incoming\":4"));
        assert!(lines[3].contains("\"cache_outcome\":\"hit\""));
        assert!(lines[3].contains("\"capture_ms\":0.000"));
        assert!(lines[3].contains("\"restore_ms\":2.300"));
    }

    /// Confirm that the `Option<u64>` outgoing/incoming fields serialize
    /// as JSON `null` when the switch has no opposite side (e.g. first
    /// switch from a freshly-launched process).
    #[test]
    fn switch_timing_null_outgoing_serializes_as_json_null() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("trace.jsonl");
        {
            let trace = RenderTrace::new(path.clone());
            trace.record(TraceEvent::SwitchTiming {
                at: Instant::now(),
                kind: SwitchKind::Session,
                outgoing: None,
                incoming: Some(1),
                capture_ms: 4.2,
                runtime_capture_ms: 0.2,
                restore_ms: 5.0,
                persist_scheduled_ms: 0.1,
                cache_outcome: CacheOutcome::Cold,
            });
        }
        let content = std::fs::read_to_string(&path).expect("read trace");
        assert!(content.contains("\"outgoing\":null"));
        assert!(content.contains("\"incoming\":1"));
        assert!(content.contains("\"kind\":\"session\""));
        assert!(content.contains("\"cache_outcome\":\"cold\""));
    }

    #[test]
    fn json_string_escapes_specials() {
        assert_eq!(json_string("hi"), "\"hi\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
    }
}

// ---------------------------------------------------------------------------
// Per-cache counters used by the FM render pipeline (TASK:phase-17/fm-render-*).
// Lightweight AtomicU64 surface so each cache wires hit/miss counts into a
// stable API without threading a `&mut` borrow through prepaint/paint.

#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct CounterSnapshot {
    pub shaped_line_cache_hits: u64,
    pub shaped_line_cache_misses: u64,
    pub row_glyph_cache_hits: u64,
    pub row_glyph_cache_misses: u64,
    pub rows_repainted: u64,
}

#[allow(dead_code)]
pub(crate) struct RenderCounters {
    pub shaped_line_cache_hits: AtomicU64,
    pub shaped_line_cache_misses: AtomicU64,
    pub row_glyph_cache_hits: AtomicU64,
    pub row_glyph_cache_misses: AtomicU64,
    pub rows_repainted: AtomicU64,
}

#[allow(dead_code)]
impl RenderCounters {
    pub const fn new() -> Self {
        Self {
            shaped_line_cache_hits: AtomicU64::new(0),
            shaped_line_cache_misses: AtomicU64::new(0),
            row_glyph_cache_hits: AtomicU64::new(0),
            row_glyph_cache_misses: AtomicU64::new(0),
            rows_repainted: AtomicU64::new(0),
        }
    }

    pub fn add_shaped_line_hits(&self, n: u64) {
        if n > 0 {
            self.shaped_line_cache_hits.fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn add_shaped_line_misses(&self, n: u64) {
        if n > 0 {
            self.shaped_line_cache_misses
                .fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn add_row_glyph_hits(&self, n: u64) {
        if n > 0 {
            self.row_glyph_cache_hits.fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn add_row_glyph_misses(&self, n: u64) {
        if n > 0 {
            self.row_glyph_cache_misses.fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn add_rows_repainted(&self, n: u64) {
        if n > 0 {
            self.rows_repainted.fetch_add(n, Ordering::Relaxed);
        }
    }

    pub fn drain(&self) -> CounterSnapshot {
        CounterSnapshot {
            shaped_line_cache_hits: self.shaped_line_cache_hits.swap(0, Ordering::Relaxed),
            shaped_line_cache_misses: self.shaped_line_cache_misses.swap(0, Ordering::Relaxed),
            row_glyph_cache_hits: self.row_glyph_cache_hits.swap(0, Ordering::Relaxed),
            row_glyph_cache_misses: self.row_glyph_cache_misses.swap(0, Ordering::Relaxed),
            rows_repainted: self.rows_repainted.swap(0, Ordering::Relaxed),
        }
    }
}

#[allow(dead_code)]
pub(crate) static COUNTERS: RenderCounters = RenderCounters::new();
