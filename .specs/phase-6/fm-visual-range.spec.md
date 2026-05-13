---
id: TASK:phase-6/fm-visual-range
type: task
status: accepted
version: 0.0.1
summary: >
  `V` enters visual-line mode — j/k extend the marked range from an
  anchor; Esc / Enter commits.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-selection#c-visual-range
---

# File-manager visual-range selection

## What ships

A new `selection_mode: SelectionMode` enum on `FileManager`:
`Single` (today's behavior) and `VisualLine { anchor: usize }`.

- `V` (shift-v) sets `selection_mode = VisualLine { anchor:
  selected_index }`.
- While in `VisualLine`, every `j` / `k` movement recomputes the
  marked range as `min..=max` of (anchor, selected_index). The
  visible marked highlight tracks the cursor live.
- `Esc` exits the mode, keeping the marks. `Enter` exits and
  commits — semantically the same.
- Any non-movement key while in `VisualLine` (other than the
  established marked-set verbs y / d / D / R / p) drops back to
  `Single` mode before being processed.

Context-gated so it doesn't shadow helix's `V` in editor panes:
the dispatch arm fires only when the FM has focus.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs):
state machine inside `navigate_up` / `navigate_down`, then the new
`V` chord. ~120 LOC.
