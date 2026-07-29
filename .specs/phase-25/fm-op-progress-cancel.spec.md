---
id: TASK:phase-25/fm-op-progress-cancel
type: task
status: accepted
version: 0.1.0
summary: >
  Wire begin/tick/finish progress and Cancel through paste, hard
  delete, bulk rename, and bulk chmod — the four ops that bypass the
  task store today.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-op-responsiveness#c-operation-progress"]
assignee:
eta:
blocked_by: []
---

# Fm op progress cancel

## Plan

Refines `REQ:codon/fm-op-responsiveness#c-operation-progress`.

The [tasks](spec:src:crates/file-manager/src/tasks.rs:1-553) machinery
(progress state, 100 ms tick throttle, history cap, cancel via shared
`Arc<AtomicBool>` wired to the notification's Cancel button) is
complete but has exactly one caller — trash delete at
[file_manager.rs:2836](spec:src:crates/file-manager/src/file_manager.rs:2836).
Wire it into:

- paste (copy/move)
  ([file_manager.rs:3281-3336](spec:src:crates/file-manager/src/file_manager.rs:3281-3336)),
- hard delete
  ([file_manager.rs:2786-2823](spec:src:crates/file-manager/src/file_manager.rs:2786-2823)),
- bulk rename
  ([file_manager.rs:3046-3101](spec:src:crates/file-manager/src/file_manager.rs:3046-3101)),
- bulk chmod
  ([file_manager.rs:2992-3044](spec:src:crates/file-manager/src/file_manager.rs:2992-3044)).

Per-entry tick + cancel check in each loop, mirroring the trash-delete
shape. Byte-level progress inside a single large file is out of scope —
it would need callbacks beneath
[copy_path](spec:src:crates/file-manager/src/file_manager.rs:4917-4946)
/ `fs::copy_recursive`. Remove the module-wide
[`#![allow(dead_code)]`](spec:src:crates/file-manager/src/tasks.rs:21)
once all call sites exist.

## Acceptance

- Each of the four operations shows a progress notification with a
  working Cancel; cancelling mid-paste keeps already-copied entries
  and reports processed/total (+ skipped).
- The task-history modal (`w`) lists entries for all five operation
  kinds.
- `#![allow(dead_code)]` is gone from `tasks.rs`; `cargo test -p
  file-manager` covers cancellation mid-loop for at least paste.
