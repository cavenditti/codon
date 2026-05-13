---
id: TASK:phase-8/fm-task-as-notifications
type: task
status: accepted
version: 0.0.1
summary: >
  Long-running fs ops emit a notification on start, replace it with a
  live-progress notification, and resolve to success/failure on
  completion.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-tasks#c-task-as-notifications
---

# File-manager tasks as notifications

## What ships

A `FmTask` abstraction wrapping every long-running fs operation
(paste, bulk delete, bulk rename, archive preview decode):

```rust
struct FmTask {
    id: FmTaskId,
    label: SharedString,    // e.g. "Pasting 12 files"
    total: usize,
    progress: Arc<AtomicUsize>,
    cancel: CancellationToken,
}
```

Thresholds for surfacing: ≥ 3 entries OR estimated byte volume
≥ 50 MB. Below that, today's silent async behavior is preserved.

The notification body is updated via `MessageNotification`'s
existing edit-in-place path (see how `HoldQuit` toasts itself).
Resolution toast lingers ~5 s with success or failure summary.

## Where it slots in

- New `crates/file-manager/src/tasks.rs` module.
- Refactor `execute_paste`, `execute_delete`, `execute_bulk_rename`
  in
  [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  to drive an `FmTask` instead of an opaque background_spawn.
- ~300 LOC.
