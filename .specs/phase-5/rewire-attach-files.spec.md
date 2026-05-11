---
id: TASK:phase-5/rewire-attach-files
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the OS-native multi-file picker at
  agent_ui::message_editor.rs:1423 with a multi-select DirPicker
  variant.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/in-app-pickers#c-rewire-attach
---

# Rewire: agent message attach files

## Callsite

[`vendor/zed/crates/agent_ui/src/message_editor.rs`](spec:src:vendor/zed/crates/agent_ui/src/message_editor.rs)
line ~1423 — "Select files to attach" multi-file prompt for the
agent message composer.

## Approach

Extend DirPicker with a `multi: bool` mode (mark/unmark entries with
space, confirm with Enter). Replace the OS dialog with this picker.
Wire the resulting `Vec<PathBuf>` into the message editor's attach
handler.

Prereq:
[TASK:phase-5/dir-picker-delegate](spec:TASK:phase-5/dir-picker-delegate)
— this task is the one that extends the delegate to multi-select,
so it lands last in the picker series.

~80 LOC including the multi-select toggle UI in the delegate.
