---
id: TASK:phase-25/fm-viewport-paging
type: task
status: accepted
version: 0.1.0
summary: >
  Derive page/half-page motion size from the measured pane height
  instead of the hard-coded 30-row constant.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-op-responsiveness#c-measured-viewport"]
assignee:
eta:
blocked_by: []
---

# Fm viewport paging

## Plan

Refines `REQ:codon/fm-op-responsiveness#c-measured-viewport`.

[visible_lines](spec:src:crates/file-manager/src/file_manager.rs:644)
is initialized to 30 and never reassigned, so
[half_page_down/up and page_down/up](spec:src:crates/file-manager/src/file_manager.rs:1373-1416)
always jump 15/30 rows regardless of actual pane height — wrong in any
split or resized window.

- Measure the visible row count during layout (the custom column
  element knows row height + bounds; the legacy `uniform_list` path
  can report its viewport item count) and write it back to the FM
  entity, keeping the last-known value as fallback before first
  layout.
- Both render paths must feed the same field.

## Acceptance

- In a half-height split, `ctrl-d` moves half the actually-visible
  rows (unit test computing lines from a given bounds height + row
  height).
- Resizing the pane updates paging behavior without requiring
  navigation first.
