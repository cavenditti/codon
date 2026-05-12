---
id: TASK:phase-5/cheatsheet-virtualization
type: task
status: accepted
version: 0.0.1
summary: >
  Virtualize the cheatsheet body so only visible rows are laid out and
  painted — currently ~200 bindings × 2 columns sit in the layout tree
  whether visible or not and scrolling feels sluggish.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/modal-shell#c-cheatsheet-contextual
---

# Virtualize the cheatsheet body

## What ships

The cheatsheet modal at
[`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)
currently renders every binding as a live element inside a plain
`v_flex` scroll region. With the contextual + 2-column layout that's
~200 bindings × 2 columns × a few nested flex wrappers per row —
2–3k layout nodes that all run layout / paint every frame.

Switch the body to either `uniform_list` (fixed-height rows) or
`list` (variable height) so only the visible rows participate in
layout. After the switch, scrolling should feel as smooth as the
file manager's directory view, which uses `uniform_list` today.

## Why this is non-trivial

- The body has a **sectioned** structure: "This pane" header → rows
  → "Codon" header → rows → "Workspace" header → rows → … . Both
  GPUI list primitives index by a flat row number, so the structure
  has to be flattened into a `Vec<RowKind>` with `RowKind::Header(…)`
  / `RowKind::Pair(left, right)` variants, and the renderer
  dispatches per-kind.
- The 2-column layout has to survive virtualization. With
  `uniform_list` the cleanest path is `RowKind::Pair` carrying both
  the left and right `BindingRow`s for that visual row — so a single
  uniform-list entry produces an `h_flex` of two side-by-side rows
  and the column-splitting math moves into the flattening step
  rather than the renderer.
- Section headers have a different height from body rows, which
  rules out `uniform_list` for the *entire* body in one pass. Two
  options:
  1. Use `list` (variable-height) end to end and let GPUI measure
     each row; simpler structure, slightly less efficient than
     uniform.
  2. Use one `uniform_list` per section, stacked inside an outer
     `v_flex`. Each section's list virtualizes its own rows; the
     outer scroll happens at the modal level. More code, but each
     row is constant-height which is the fast path.
- Striping is per-row-in-column today. Under virtualization the
  pair index is the natural striping anchor — store
  `pair_index_in_section` on `RowKind::Pair` so the row background
  doesn't shift on scroll.

## File anchors

- [`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)
  — the only file in scope.
- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  — uses `uniform_list` with a scroll handle and is the closest
  in-codon template for the GPUI plumbing.

## Acceptance

- Open the cheatsheet on a pane with many bindings (editor or
  terminal) and scroll: visual frame rate is comparable to the file
  manager scrolling 200+ rows.
- Section headers and 2-column striping still render correctly.
- "This pane" empty-state hint still works (single muted line, no
  shift in the layout below).

Effort: medium. ~120-180 LOC for the flattening + dispatched
renderer, plus the existing render code becomes leaner since each
row renderer no longer has to know about its siblings.
