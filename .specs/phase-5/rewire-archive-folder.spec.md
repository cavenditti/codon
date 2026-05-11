---
id: TASK:phase-5/rewire-archive-folder
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the OS-native folder picker at
  agent_ui::threads_archive_view.rs:1237 with the DirPicker-backed
  in-app modal.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/in-app-pickers#c-rewire-archive
---

# Rewire: agent thread archive folder

## Callsite

[`vendor/zed/crates/agent_ui/src/threads_archive_view.rs`](spec:src:vendor/zed/crates/agent_ui/src/threads_archive_view.rs)
line ~1237 — "Open local folder" prompt used when archiving / loading
agent threads from a directory.

## Approach

Replace the OS dialog with a DirPicker modal configured for
directory-only selection. Wire the `DirSelected` event into the
archive-load handler.

Prereq:
[TASK:phase-5/dir-picker-delegate](spec:TASK:phase-5/dir-picker-delegate).

~60–80 LOC.
