---
id: TASK:phase-25/fm-resort-in-memory
type: task
status: accepted
version: 0.1.0
summary: >
  Re-sort the loaded listing in memory on sort changes: drop sort
  options from the DirCache key, precompute comparator keys, and stat
  each entry only once per read.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-listing-model#c-in-memory-resort"]
assignee:
eta:
blocked_by: []
---

# Fm resort in memory

## Plan

Refines `REQ:codon/fm-listing-model#c-in-memory-resort`.

Today `sort`/`reverse` ride in
[ReadDirOptions](spec:src:crates/file-manager/src/file_manager.rs:5115-5119)
and therefore in the `DirCache` key;
[set_sort_mode](spec:src:crates/file-manager/src/file_manager.rs:4435),
[toggle_sort_reverse](spec:src:crates/file-manager/src/file_manager.rs:4442),
and [set_ranger_sort](spec:src:crates/file-manager/src/file_manager.rs:4457)
all call `reload_entries`, and
[DirCache::store](spec:src:crates/file-manager/src/file_manager.rs:5178)
retains only one options-variant per path — so every sort toggle is a
guaranteed cache miss that re-reads and re-stats both columns and wipes
marks via
[prepare_reload](spec:src:crates/file-manager/src/file_manager.rs:770-783).

- Key `DirCache` on path + non-sort options only; apply sorting after
  fetch via the existing pure
  [sort_entries](spec:src:crates/file-manager/src/file_manager.rs:5494)
  on the in-memory current + parent listings.
- Precompute a case-folded name key and extension key in `EntryLabels`
  at read time; remove the per-comparison `to_lowercase` in
  [compare_in_group](spec:src:crates/file-manager/src/file_manager.rs:5526)
  and the allocations in
  [extension_key](spec:src:crates/file-manager/src/file_manager.rs:5533-5547).
- Collapse the double syscall in
  [read_dir_sync](spec:src:crates/file-manager/src/file_manager.rs:5285-5286)
  — `metadata()` already carries the file type.
- Sort changes must not route through `prepare_reload` (cursor and
  marks stay; final marks semantics land in
  `TASK:phase-25/fm-marks-by-path`).

## Acceptance

- Toggling sort mode or direction on a cached listing performs no
  `read_dir` and no per-entry stat (unit test via an injected read
  counter or `DirCache` hit assertions) and preserves the cursor entry.
- The comparator signature takes precomputed keys; no
  `to_lowercase`/`String` allocation per comparison remains in the sort
  path.
- `cargo test -p file-manager` covers: cache key excludes sort
  options; in-memory resort matches `read_dir`-fresh ordering for every
  `SortMode` × `reverse` combination.
