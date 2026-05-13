---
id: TASK:phase-5/fm-copy-paste
type: task
status: accepted
version: 0.0.1
summary: >
  Modal y / d / p operations in file manager — yank marks for copy,
  delete-mark marks for move, paste resolves into current directory.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-enhancements#c-copy-paste
---

# File copy / move via y / d / p

## What ships

In file-manager Normal mode, on the current entry (or the marked set
if any):

- `y` (yank) — store paths in a "copy clipboard" (codon-local, not the
  OS clipboard)
- `d` (delete, repurposed) — store paths in a "cut clipboard"
- `p` (paste) — for each clipboard entry, `fs::copy` (if yank) or
  `fs::rename` (if cut) into the current directory

Conflicts trigger a numbered suffix (`foo.txt` → `foo (2).txt`) by
default; `P` (shift-p) prompts before overwrite.

## Where it slots in

- Existing key handler in
  [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs).
  The `CopyMarked` / `MoveMarked` actions are already declared at the
  top of the file but unimplemented.
- File operations go through `fs::Fs` (the trait codon already takes
  in `FileManager::new`) so the same async path works on remote
  filesystems eventually.

## Approach

Add a `FmClipboard` enum (`Yank(Vec<PathBuf>)` / `Cut(Vec<PathBuf>)`)
to a local global or to `FileManager` itself. The paste handler is the
substantive bit — ~100–150 LOC including conflict resolution.

Note the `d` overload: it currently deletes the entry. Move the
deletion to `D` (shift-d) so single-tap `d` is the cut-mark.
