---
id: TASK:phase-8/fm-task-cancel
type: task
status: accepted
version: 0.0.1
summary: >
  Live-progress notifications carry an `x` action that cooperatively
  cancels the task between per-entry chunks.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-tasks#c-task-cancel
---

# File-manager task cancellation

## What ships

The progress notification renders an `x` clickable / keyboard-
accessible button. Pressing it calls `FmTask::cancel.cancel()`.

Each fs op checks `cancel.is_cancelled()` between entry-level
chunks (per-file in paste / delete / bulk rename) and exits the
loop. Already-completed work is preserved; the resolution toast
states "cancelled after N of M".

## Where it slots in

- `MessageNotification` action API — check how
  [`vendor/zed/crates/notifications/`](spec:src:vendor/zed/crates/notifications/)
  surfaces clickable actions on a notification; reuse that surface
  if available, else add a small notification variant.
- Loop bodies in `execute_*` get a `cancel.is_cancelled()` check.
- ~150 LOC.
