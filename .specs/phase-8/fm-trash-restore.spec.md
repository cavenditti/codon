---
id: TASK:phase-8/fm-trash-restore
type: task
status: accepted
version: 0.0.1
summary: >
  Enter restores the highlighted trash entry to its original location;
  Space marks for bulk restore.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-trash#c-trash-restore
---

# File-manager trash restore

## What ships

Inside the `T` modal (TASK:phase-8/fm-trash-list):

- `Enter` on a single highlighted entry calls
  `trash::os_limited::restore_all(&[entry])`. Target-path
  conflicts surface the numbered-suffix prompt the paste handler
  already uses.
- `Space` toggles a per-row mark (same `marked: BTreeSet<usize>`
  shape codon's FM uses). When marks exist, `Enter` restores
  every marked entry in one batch.

Errors (e.g. parent directory no longer exists) surface via
`surface_error`; partial successes proceed with the rest of the
batch.

## Where it slots in

`TrashListModal` from TASK:phase-8/fm-trash-list. ~120 LOC.
