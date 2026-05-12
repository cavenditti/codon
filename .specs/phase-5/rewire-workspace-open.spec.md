---
id: TASK:phase-5/rewire-workspace-open
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the OS-native open-file dialog at workspace.rs:2972 with
  the DirPicker-backed in-app modal.
owners: [carlo]
progress: done
refines:
  - REQ:codon/in-app-pickers#c-rewire-workspace
---

# Rewire: workspace open-file dialog

## Callsite

[`vendor/zed/crates/workspace/src/workspace.rs`](spec:src:vendor/zed/crates/workspace/src/workspace.rs)
line ~2972 — `cx.prompt_for_paths(...)` for the general
"Open File / Folder" workspace action.

## Approach

Replace the prompt with `DirPicker` opened as a workspace modal. Wire
the `DirSelected` event into the same path the existing dialog used
(probably `workspace::open_paths`).

Prereq:
[TASK:phase-5/dir-picker-delegate](spec:TASK:phase-5/dir-picker-delegate).

~60–80 LOC. Run codon, hit the workspace open-file binding, confirm
the in-app picker appears and selecting a path opens it in the active
pane.
