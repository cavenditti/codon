---
id: TASK:phase-6/fm-select-all-invert
type: task
status: accepted
version: 0.0.1
summary: >
  `ctrl-a` selects every currently-visible entry; `ctrl-r` inverts
  the mark set against the same visible window.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-selection#c-select-all-invert
---

# File-manager select-all / invert

## What ships

- `ctrl-a` sets `marked` to every index in the currently-visible
  `entries` (respects `.` hidden / `zg` gitignore / `f` filter — the
  list the user is actually looking at).
- `ctrl-r` flips each visible-index's membership in `marked` (the
  set-symmetric-difference against the visible indices). Entries
  outside the visible window keep their existing state.

Both are no-op when the FM has no entries.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs):
dispatch arms for `ctrl-a` and `ctrl-r`. ~40 LOC.
