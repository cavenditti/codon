---
id: TASK:phase-6/fm-line-modes
type: task
status: accepted
version: 0.0.1
summary: >
  Per-entry metadata column — cycle through None / Size / Mtime /
  Permissions / Owner with `M`.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-sort-display#c-line-modes
---

# File-manager line modes

## What ships

A `LineMode` enum cycled with `M` (shift-m). Each variant appends
a right-justified text column to every entry's row:

- `None` — current behavior (no extra column).
- `Size` — human-readable file size (reuse `human_size` already
  in [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)).
- `Mtime` — short relative time ("2h ago", "3d ago") matching
  the helper added in TASK:phase-5/session-overview.
- `Permissions` — unix mode in symbolic form (`drwxr-xr-x`).
- `Owner` — `user:group`.

Persisted via codon-config (`[file_manager] line_mode = "..."`).

## Where it slots in

- Add `line_mode` to `FileManager` state.
- Extend `DirEntry` to cache the metadata columns when read.
- Update [`crates/file-manager/src/view.rs`](spec:src:crates/file-manager/src/view.rs)
  middle-column row renderer to append the metadata cell when
  non-None. Right-align in a fixed-width slot to keep the chord
  column stable.
- ~150 LOC.
