//! Render-trace harness.
//!
//! A feature-gated, per-process JSONL recorder that captures three event
//! kinds emitted from the FM render pipeline:
//!
//! - `KeypressDispatched` — a key reached the FM input handler.
//! - `FramePainted` — the FM render closure returned (provisional;
//!   accurate prepaint/paint/draw split arrives with
//!   `TASK:phase-17/fm-render-custom-row`).
//! - `PreviewUpgraded` — the FM upgraded the preview column from the
//!   lightweight snapshot to the real `EditorElement`.
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
    sync::{Mutex, OnceLock},
    time::Instant,
};

use gpui::SharedString;

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
        | TraceEvent::PreviewUpgraded { at, .. } => *at,
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
        }
        let content = std::fs::read_to_string(&path).expect("read trace");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3, "one line per event");
        assert!(lines[0].contains("\"t\":\"keypress\""));
        assert!(lines[0].contains("\"key\":\"j\""));
        assert!(lines[1].contains("\"t\":\"frame_painted\""));
        assert!(lines[1].contains("\"rows_painted\":30"));
        assert!(lines[2].contains("\"t\":\"preview_upgraded\""));
        assert!(lines[2].contains("/tmp/foo.rs"));
    }

    #[test]
    fn json_string_escapes_specials() {
        assert_eq!(json_string("hi"), "\"hi\"");
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
    }
}
