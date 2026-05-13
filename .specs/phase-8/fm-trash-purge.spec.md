---
id: TASK:phase-8/fm-trash-purge
type: task
status: accepted
version: 0.0.1
summary: >
  `X` permanently deletes — from inside the trash modal (purge) or on
  a live file (skip-trash delete) with single-prompt confirm.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-trash#c-trash-purge
---

# File-manager permanent delete / purge

## What ships

Two surfaces under one chord:

- Inside `T` modal (TASK:phase-8/fm-trash-list): `X` (shift-x)
  prompts "permanently delete N entries? y/N", then calls
  `trash::os_limited::purge_all` on the highlighted / marked set.
- In the main FM listing: `X` prompts the same way, then deletes
  bypassing the trash via `fs::Fs::remove_file` /
  `remove_dir_all`. This is the "I'm sure, skip trash" path
  distinct from `D` (which sends to trash).

Both share the existing `PendingInput::ConfirmDeleteMarked`
prompt format added by phase-5/fm-bulk-ops, with a new field
`skip_trash: bool` distinguishing the path taken.

## Where it slots in

- `TrashListModal` purge branch.
- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  Normal-mode `X` dispatch.
- ~100 LOC.
