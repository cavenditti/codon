---
id: TASK:phase-25/fm-marks-by-path
type: task
status: accepted
version: 0.1.0
summary: >
  Key the marked set by canonical path so marks survive reload,
  re-sort, filter changes, and watcher deltas.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-listing-model#c-path-keyed-marks"]
assignee:
eta:
blocked_by: []
---

# Fm marks by path

## Plan

Refines `REQ:codon/fm-listing-model#c-path-keyed-marks`.

The marked set is
[marked: BTreeSet<usize>](spec:src:crates/file-manager/src/file_manager.rs:393)
(with
[visual_anchor](spec:src:crates/file-manager/src/file_manager.rs:430)),
wiped by
[prepare_reload](spec:src:crates/file-manager/src/file_manager.rs:771)
on every listing change and by
[apply_filter](spec:src:crates/file-manager/src/file_manager.rs:2742).
The index-collision hazard for parent/preview columns is documented at
[render_entry_row](spec:src:crates/file-manager/src/view.rs:61-66);
the per-frame `BTreeSet → HashSet` conversion sits at
[view.rs:696](spec:src:crates/file-manager/src/view.rs:696); operations
already resolve marks to paths at action time via
[current_targets](spec:src:crates/file-manager/src/file_manager.rs:1966-1979).

- Store marks as a path-keyed set; derive a per-listing-generation
  index lookup (bitmap or `HashSet<usize>`) for O(1) row checks so
  render cost stays O(visible).
- On reload / re-sort / filter / watcher delta, retain marks whose
  paths survive in the new listing and drop the rest.
- Visual-range marking
  ([refresh_visual_marks](spec:src:crates/file-manager/src/file_manager.rs:1850-1861))
  keeps operating on row indices but commits paths.
- The marked count/size footer reflects the surviving set.

## Acceptance

- Marks survive a sort toggle, filter apply + clear, and an explicit
  `Reload`; a marked file deleted externally disappears from the
  marked set on the next listing refresh without disturbing other
  marks (unit tests).
- Parent/preview columns never highlight rows by index collision
  (regression test for the `view.rs:61-66` scenario).
- Per-frame mark lookup remains O(visible rows); no full-set scan per
  row.
