---
id: REQ:codon/fm-op-responsiveness
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Long-running file operations report progress and honor cancellation,
  no keystroke blocks the UI thread on disk writes, page motions track
  the real pane height, and preview I/O is bounded by a byte budget.
owners: [carlo]
refines: []
categorized_under: []
---

# Responsive file operations and input

:::{requirement id="fm-op-responsiveness" level="MUST"}
The system MUST:

- {#c-operation-progress} report every multi-entry mutating operation —
  paste (copy/move), hard delete, bulk rename, and bulk chmod, joining
  the existing trash-delete integration — through the FM task store
  with begin/tick/finish and a working Cancel. Cancellation MUST leave
  a consistent listing and report how many entries completed.
- {#c-measured-viewport} derive page and half-page motions from the
  measured pane height. The motions MUST be correct in splits and
  after window resizes; no hard-coded row count.
- {#c-async-config-writes} persist preferences and bookmarks without
  blocking the UI thread. Rapid repeated changes (key auto-repeat)
  MUST coalesce into a debounced single write with last-write-wins
  semantics, and pending writes MUST flush before process exit.
- {#c-preview-io-budget} bound preview I/O: text preview MUST read at
  most the preview byte cap from disk (a head read, never
  whole-file-then-discard), the editor-upgrade dwell MUST be a named
  constant (MAY be user-configurable), and the upgraded-preview cache
  SHOULD hold at least two entries so alternating between adjacent
  files does not rebuild the editor each time.
:::
