---
id: TASK:phase-17/switch-overview-defer-capture
type: task
status: draft
version: 0.0.1
summary: >
  `SessionOverview` and `WindowOverview` action handlers MUST stop
  calling `swap::capture` on the modal-open path. The modal sources
  its layout summary from the last cached `Window::layout` snapshot
  and captures fresh state on-demand when a tile needs it.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/perf-switch#c-overview-defer-capture
aspects: [overview-modal, defer-capture, on-demand]
---

# Defer swap::capture on overview open

## What changes

In `crates/codon-session/src/actions.rs`, the
`SessionOverview` and `WindowOverview` action handlers
(roughly lines 270–290) currently start with:

```rust
let snapshot = swap::capture(workspace, window, cx);
```

before constructing the modal entity. This runs the full
layout walk regardless of whether the user keeps the modal
open.

Replace with a lazy capture path:

```rust
// Pull the last persisted snapshot for the active window. It is
// kept fresh by the runtime cache eviction hook
// (phase-17/switch-skip-capture-on-cache-hit), so the lag is
// bounded by one cache eviction.
let snapshot = registry
    .get(active_session_id)
    .and_then(|s| s.active().and_then(|w| w.layout.clone()));

// The modal calls back into this function only when a tile
// representing the current window needs *more* than the
// snapshot (e.g. live focus indicator).
let capture_on_demand = Arc::new({
    let weak = workspace.weak_handle();
    move |cx: &mut App| -> Option<LayoutSnapshot> {
        weak.update(cx, |ws, cx| swap::capture(ws, window_handle, cx)).ok()
    }
});
```

The overview modal (`crates/codon-session/src/overview.rs`)
gains an optional `capture_on_demand` callback field; today's
unconditional capture becomes a no-op when the snapshot is
already adequate.

## Why this clause

Two observed pain points in the profile:

1. The overview modal is the first thing on `prefix s O` and
   `prefix w O` — typing the chord and immediately dismissing
   still pays the full `capture` cost.
2. For large layouts (8+ panes, multiple items per pane), the
   capture is on the order of milliseconds and is visible as
   chord lag.

The stored `Window::layout` is sufficient for the modal's
visual summary; live freshness is only needed for the *active*
tile, and even then only when its content depends on
post-snapshot state.

## Verification

- New test
  `session_overview_open_does_not_call_capture` instruments a
  counter inside `swap::capture` and asserts it is not invoked
  when opening + dismissing the modal without interacting.
- Existing overview tests pass unchanged — the modal's visual
  output is identical for the steady-state case.
- `cargo clippy -p codon-session` is clean.

## Done when

- Both overview action handlers source the snapshot from the
  registry rather than calling `swap::capture`.
- The modal's `capture_on_demand` callback path exists for the
  fresh-snapshot need.
- `spec lint` is at zero errors.
