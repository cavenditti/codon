---
id: TASK:phase-8/fm-task-history
type: task
status: accepted
version: 0.0.1
summary: >
  `w` opens a modal listing the last 50 fs tasks (active + recent)
  with re-emit-as-notification per row.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-tasks#c-task-history
---

# File-manager task history modal

## What ships

`w` (codon_fm::TaskHistory action) opens a modal listing the
last 50 `FmTask` records — both currently-running and recently-
completed:

```
Pasting 12 files        running    00:04   [cancel]
Deleting 3 files        success    00:01
Bulk rename 5 files     cancelled  00:00   restore?
```

Enter on a finished task re-emits its resolution notification
(useful if the user dismissed it accidentally). Enter on a
running task focuses the live notification.

Backing store is in-memory only; cleared on quit. The store lives
beside the `FmTask` machinery from
TASK:phase-8/fm-task-as-notifications.

## Where it slots in

- Workspace-scoped action + modal, mirroring the
  `SessionOverviewModal` pattern.
- Reuses `gpui::list` for the row virtualization (50 rows is
  fine without it, but consistency is cheap).
- ~250 LOC.
