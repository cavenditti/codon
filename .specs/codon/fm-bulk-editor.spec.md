---
id: REQ:codon/fm-bulk-editor
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  $EDITOR-driven bulk verbs — bulk rename via a temp buffer (yazi's
  `:` flow) complementing the existing `R` pattern rename, plus bulk
  chmod.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-8]
---

# File manager bulk verbs via $EDITOR

Phase 5 shipped `R` for bulk-rename-by-pattern (`{}` counter). That
covers the structured case (numbered series, simple suffix swaps).
For ad-hoc renaming where each new name is different, yazi's
$EDITOR flow is more ergonomic: open a buffer, edit names, save,
codon applies the rename.

:::{requirement id="fm-bulk-editor" level="MAY"}
The file manager SHOULD support:

- {#c-bulk-rename-editor} `cw` (change-word, in vim spirit) writes
  every marked entry's *current name* (one per line) into a
  workspace buffer titled `Bulk rename — N files`. On save and
  close, codon diffs original→edited names line-for-line and
  applies the resulting renames atomically (collect into one
  task; rollback half-applied changes on first error).
  Line-count mismatch surfaces an error and applies nothing.
  Coexists with the existing pattern-based `R`.
- {#c-bulk-chmod} `cm` opens an input bar for an octal (`755`)
  or symbolic (`u+x`) mode. Applies via `fs::Fs::set_permissions`
  to every marked entry. No-op + toast on Windows.
:::
