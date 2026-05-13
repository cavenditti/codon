---
id: REQ:codon/fm-selection
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Visual-range selection mode, select-all / invert, clear-marks — the
  selection capabilities yazi gives via `v`/`V`/`Ctrl-A`/`Ctrl-R`.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-6]
---

# File manager selection

Today `v` toggles the mark on the current entry. That covers
single-entry verbs but not contiguous-range or whole-listing
selection. This requirement adds the remaining selection modes.

:::{requirement id="fm-selection" level="SHOULD"}
The file manager SHOULD support:

- {#c-visual-range} `V` (shift-v) enters visual-line mode. The
  modal anchor is the cursor at entry. Subsequent `j` / `k`
  movement extends or shrinks the marked range from the anchor.
  `Esc` or `Enter` commits the resulting marks; the new state
  becomes the input to existing y / d / p / D / R verbs.
  Conflicts with vim's helix line-select must be context-gated
  (this mode only applies inside `FileManager`).
- {#c-select-all-invert} `ctrl-a` marks every entry currently
  visible (respecting `.` hidden / `zg` gitignore / `f` filter).
  `ctrl-r` inverts the mark set against the same visible window.
- {#c-clear-marks} `uv` (vim-style: unmark + visual) clears all
  marks. Bound separately from the existing single-entry toggle so
  `v` retains its current meaning.
:::
