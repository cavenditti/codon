---
id: TASK:phase-6/fm-bookmarks
type: task
status: accepted
version: 0.0.1
summary: >
  Vi-style global bookmarks for the file manager — `m<letter>` saves,
  `'<letter>` jumps. 26 slots, persisted across launches.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-nav-extras#c-bookmarks
---

# File-manager bookmarks

## What ships

A 26-slot bookmark table (`a`-`z`), persisted to
`~/.local/state/codon/fm-bookmarks.toml`, accessible from every
codon launch:

- `m` (no shift) followed by a letter: save `current_dir` into
  that slot. Existing value is replaced silently.
- `'` (apostrophe) followed by a letter: set `current_dir` to
  the bookmarked path. Surfaces a toast if the path no longer
  exists.

Both `m` and `'` are chord-style: they expect a single character
within the existing 5-s GPUI chord timeout (`set_keystroke_chord_timeout`,
already set by codon-keymap).

## Where it slots in

- New module `crates/file-manager/src/bookmarks.rs` — defines
  `BookmarkStore { slots: [Option<PathBuf>; 26] }` + load/save
  via `serde + toml`. State dir resolution mirrors codon-config's
  `codon_state_dir()` helper (add it there if missing).
- Wire `m<letter>` / `'<letter>` as chord predicates in the FM
  key dispatch. Chord state is a `pending_chord: Option<char>`
  on `FileManager`.
- ~150 LOC including the persistence module.
