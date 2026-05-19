---
id: TASK:phase-17/switch-budget-harness
type: task
status: draft
version: 0.0.1
summary: >
  Extend the render-trace harness (`phase-17/fm-render-frame-budget`)
  with a `SwitchTiming` event type that captures per-switch capture /
  restore / persist durations, plus a scripted-switch replay. Acts as
  the acceptance gate for the switch-perf clauses.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/perf-switch#c-switch-budget-harness
aspects: [instrumentation, switch-timing, acceptance-gate]
---

# Switch-timing budget harness

## What changes

Build on the JSONL render-trace introduced by
[`TASK:phase-17/fm-render-frame-budget`](spec:TASK:phase-17/fm-render-frame-budget).
Add the event:

```rust
enum TraceEvent {
    // ... existing variants ...
    SwitchTiming {
        at: Instant,
        kind: SwitchKind,                 // Window | Session
        outgoing: Option<WindowId>,
        incoming: Option<WindowId>,
        capture_ms: f32,                  // 0.0 on cache hit
        runtime_capture_ms: f32,
        restore_ms: f32,
        persist_scheduled_ms: f32,        // time spent in scheduler.mark_dirty
        cache_outcome: CacheOutcome,      // Hit | Miss | Cold
    }
}
```

Hook the timing into:
- `cycle_window` (window switch).
- `attach_session` (session switch).
- `restore_center_root` (vendored Zed — small additive timing
  callback, see below).

The vendored-Zed side gets a thin callback:

```rust
// codon_bridge.rs
pub fn set_restore_timing_callback(
    cb: fn(restore_ms: f32, cache_outcome: CacheOutcome)
);
```

`codon-session::init` installs the callback so the vendored
crate doesn't import a codon type. Same pattern as the existing
codon pane-kind registry.

`scripts/render-trace-replay.fish` gains a `--switch-cycle`
mode:

```text
loop 60s:
    press prefix-Tab (300 ms gap)         # cycle window
    every 5 cycles, press prefix-s prefix-Tab  # session switch
```

## Acceptance gate

On the reference Apple-Silicon device, the scripted switch-cycle
trace MUST report:

| Metric | Target |
|---|---|
| `SwitchTiming.capture_ms` p95 (cache hit) | = 0 (skipped by `c-skip-capture-on-cache-hit`) |
| `SwitchTiming.capture_ms` p95 (cache miss) | ≤ 6 ms |
| `SwitchTiming.restore_ms` p95 (cache hit) | ≤ 3 ms (after `c-restore-pane-set-hashmap` + `c-restore-skip-notify`) |
| Total `cycle_window` p95 (cache hit) | ≤ 8 ms |
| Total `attach_session` p95 (cache hit) | ≤ 12 ms |
| Persist-task spawn count over 60 s | ≤ 30 (down from ~200 with default 300 ms switch cadence) |

## Verification

- The `SwitchTiming` event appears in the JSONL output for each
  switch.
- `scripts/render-trace-report.py --kind switch trace.jsonl`
  prints percentile tables matching the budget.
- The harness imposes < 1% overhead when enabled (same
  microbench approach as the FM render harness).

## Done when

- `SwitchTiming` is emitted at all switch boundaries.
- The scripted replay drives both window- and session-switch
  cadences.
- The reference trace meets the acceptance targets.
- `spec lint` is at zero errors.
