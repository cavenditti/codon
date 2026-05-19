---
id: TASK:phase-17/fm-render-frame-budget
type: task
status: draft
version: 0.0.1
summary: >
  Feature-gated `--render-trace` harness that logs per-frame
  prepaint / paint / draw timings plus `keypress → painted`
  deltas to a JSON file under `$CODON_LOG_DIR/render-trace/`.
  Acts as the acceptance gate for the rest of phase-17 — the
  FM redraw cycle must measure ≤ 5 ms / frame at p95, ≤ 3 ms
  typical with cache hits.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-render#c-frame-budget-harness
aspects: [instrumentation, render-trace, acceptance-gate]
---

# Render-trace frame-budget harness

## What changes

Add the harness in three pieces:

1. **CLI flag.** `apps/codon/src/main.rs` parses
   `--render-trace[=file]`. When set, installs the trace
   collector before `Application::run`.
2. **Settings field.** `[diagnostics] render_trace = true` in
   `codon.toml`, with optional `render_trace_path = "..."`.
   The CLI flag takes precedence over the settings.
3. **Collector.** New module
   `crates/file-manager/src/render/trace.rs`:

   ```rust
   pub(crate) struct RenderTrace {
       events: Mutex<Vec<TraceEvent>>,
       path: PathBuf,
   }

   enum TraceEvent {
       KeypressDispatched { at: Instant, key: SharedString },
       FramePainted       { at: Instant, prepaint_ms: f32, paint_ms: f32, draw_ms: f32, rows_painted: u32, cache_hits: u32, cache_misses: u32 },
       PreviewUpgraded    { at: Instant, path: PathBuf },
   }

   impl RenderTrace {
       pub fn record(event: TraceEvent) { ... }
       pub fn flush_on_drop(self);
   }
   ```

The custom Elements hook into the collector at their
`prepaint` / `paint` boundaries using
`Instant::now()` deltas; the cost of the trace itself is
~50 ns per event and is acceptable as overhead.

Output format (JSONL, one event per line) chosen for grep-ability:

```jsonl
{"t":"keypress","at_ms":12.4,"key":"j"}
{"t":"frame_painted","at_ms":18.1,"prepaint_ms":0.4,"paint_ms":2.1,"draw_ms":1.6,"rows_painted":30,"cache_hits":28,"cache_misses":2}
```

A small `scripts/render-trace-report.py` (added to the repo)
reads a trace file and prints p50/p95/p99 distributions for
each metric.

## Acceptance gate

The phase ships when, on the reference Apple-Silicon
M-series device (M2 Pro 16-core / 32 GB / 120 Hz display, the
device used for the post-async-git-status samply runs), the
trace from a scripted 60-second navigation session over a
500-entry tree reports:

| Metric | Target |
|---|---|
| Frame painted (prepaint+paint+draw) p50 | ≤ 3 ms |
| Frame painted p95 | ≤ 5 ms |
| Keypress → painted p95 | ≤ 16 ms (one vsync at 60 Hz) |
| Row-glyph cache hit rate | ≥ 95% |
| Shaped-line cache hit rate | ≥ 90% |

The scripted navigation:

```text
loop 60s:
    press j (60 ms gap)
    press k (60 ms gap)
    press h (200 ms gap)
    press l (200 ms gap)
```

The `scripts/render-trace-replay.fish` (added in this task)
drives the keypress sequence via `cliclick` or equivalent.

## Why this clause

Without measurement there is no rendering pipeline phase. The
trace gives both the per-task signal (does
`fm-render-custom-row` actually drop prepaint cost?) and the
overall acceptance bar. It also catches regressions in later
phases: the harness can stay shipped behind the settings
flag and act as a perf-canary in development.

## Verification

- The flag and settings field work end-to-end; running
  `codon --render-trace=/tmp/codon-trace.jsonl` followed by a
  scripted navigation produces a non-empty JSONL file.
- `scripts/render-trace-report.py /tmp/codon-trace.jsonl`
  prints sensible percentiles.
- The harness imposes < 1% steady-state overhead when
  enabled (measured by enabling/disabling and comparing
  baseline frame times on an idle file manager).

## Done when

- `--render-trace` is wired in `apps/codon/src/main.rs`.
- The collector is invoked from the custom row / column
  Elements.
- The reference scripted-navigation trace meets the
  acceptance targets above (this is what gates phase-17
  closing).
- `spec lint` is at zero errors.
