---
id: TASK:phase-6/fm-sort-modes
type: task
status: accepted
version: 0.0.1
summary: >
  `SortMode` enum on FileManager — Name / Size / Mtime / Btime /
  Extension / Random / Natural, selectable via `,n` `,s` `,m` `,b`
  `,e` `,r` `,N` chords.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-sort-display#c-sort-modes
---

# File-manager sort modes

## What ships

`SortMode` enum + per-mode comparator. The active mode is stored
on `FileManager` and re-applied on `reload_entries`. Default
remains the current behavior (dirs-first, name-ascending,
case-insensitive) — labelled `SortMode::Name`.

Comparators:
- Name: case-insensitive natural-ish (`a < B < c`).
- Size: file size from `Metadata::len`.
- Mtime / Btime: `Metadata::modified()` / `created()`. Btime falls
  back to mtime on filesystems that don't expose it.
- Extension: lowercased extension, then name as tiebreak.
- Random: `rand::shuffle` once per `,r` press (not on every reload).
- Natural: numeric-aware (`file2 < file10`); use `natord` if not
  pulled in transitively, else write a small comparator inline.

Persisted via codon-config writeback (TOML `[file_manager] sort = "..."`).

## Where it slots in

- `read_dir_sync` in
  [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  currently hard-codes the comparator at line ~1348. Parameterise.
- Add `,` as a chord prefix in the FM key dispatch; subsequent
  letter chooses the mode.
- ~200 LOC.
