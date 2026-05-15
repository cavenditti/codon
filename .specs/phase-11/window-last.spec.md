---
id: TASK:phase-11/window-last
type: task
status: accepted
version: 0.1.0
summary: >
  WindowLast — toggle to the previously-active window in the same
  session (tmux `prefix l`).
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-last
categorized_under: [TOPIC:topics/phase-11]
---

# Window-last toggle

## What ships

- New action `codon_session::WindowLast` (unit struct via `actions!`).
- New field `Session::previous_window: Option<usize>`. Initialized
  `None`; bumped to the *outgoing* window index on every switch path:
  `cycle_window` (Next/Prev), `switch_to_window` (status-bar click,
  picker, overview), and `handle_window_goto`.
- The action looks up `previous_window`, validates the index is in
  range (defensive: registry could have shed windows since it was
  set), and dispatches the same `switch_to_window` path used by
  every other window-switch surface.
- Bound in the default keymap. Two-key motion: `cmd-k l`. The
  three-key menu chord `cmd-k shift-w shift-l` also works for
  discoverability under the "windows menu" prefix.

## Why this shape

Storing `previous_window` on `Session` (not `WindowRuntimeCache`)
means the toggle survives restart, which matches the rest of the
session model. `Option<usize>` over a `WindowId` keeps the field
trivially serde-able and dodges the question of what to do when a
WindowId disappears — out-of-range indices are no-ops with a debug
log.

Effort: small. ~40 LOC plus keymap.
