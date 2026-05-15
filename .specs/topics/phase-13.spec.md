---
id: TOPIC:topics/phase-13
type: topic
status: draft
version: 0.0.1
summary: >
  Status-bar overhaul: rewire the bottom bar into a real three-zone
  modeline — global state on the left, pane context in the centre,
  meta + dynamic messages on the right — with mode and window
  indicators load-bearing under any width.
owners: [carlo]
---

# Phase 13 — Status bar overhaul

Codon inherits Zed's status bar as a single flat row built from two
vecs (`left_items` / `right_items`) in
`vendor/zed/crates/workspace/src/status_bar.rs`. Today seventeen
indicators are hung off those vecs in roughly the order someone
needed them — mode and session sit next to encoding and image-info,
and `project_info` is even pushed *left* mid-`add_right_item` block
in `apps/codon/src/zed.rs:599`. The shell reads like a buffet, not a
modeline.

Phase 13 reorganises the bar around three explicit zones:

```
LEFT    mode  ·  session  ·  windows
CENTER  git_branch  ·  file_or_cwd  ·  language  ·  cursor
RIGHT   activity_indicator  ·  diagnostics  ·  lsp  ·  merge_conflict
        ·  edit_prediction  ·  toolchain  ·  encoding  ·  line_ending
        ·  project_info  ·  image_info
```

The mental model:

- **Left = global state.** The codon-identity row. What mode am I
  in, what session am I attached to, which window am I on. Fixed
  positions; muscle memory works. These three items MUST NEVER
  hide or truncate, even when the bar is otherwise overflowing.
- **Centre = "what is this pane".** Reads left-to-right as a
  sentence describing the focused pane: on branch X, file (or cwd,
  or agent verb) Y, language L, at line:col. Truncates under width
  pressure — the file/cwd segment loses middle characters first.
- **Right = ambient + transient.** Diagnostics, LSP, build/format
  progress, and slow-changing meta (encoding, line-ending,
  toolchain, edit-prediction). `activity_indicator` is anchored
  rightmost because background-build text is the loudest dynamic
  signal and reads best at the edge of the eye.

`search_button` is removed. Search is a verb reachable from the
keymap and command palette; it is not status.

Refining requirements:

- [REQ:codon/status-bar](spec:REQ:codon/status-bar) — clauses
  `#c-three-zones`, `#c-left-protected`, `#c-centre-pane-context`,
  `#c-right-meta-and-dynamic`, `#c-collapse-policy`,
  `#c-no-search-button`, `#c-vendored-zone-api`, `#c-git-branch-item`,
  `#c-pane-context-item`.
