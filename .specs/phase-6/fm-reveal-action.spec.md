---
id: TASK:phase-6/fm-reveal-action
type: task
status: accepted
version: 0.0.1
summary: >
  `codon_fm::Reveal(PathBuf)` action — navigates the file manager to
  a path's parent and selects the entry. Callable from anywhere.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-nav-extras#c-reveal-file
---

# File-manager reveal-by-path action

## What ships

A workspace-scoped action `codon_fm::Reveal { path: PathBuf }`
(derive-Action). The handler:

1. Opens the most-recently-active FM pane (or creates one via
   `codon_session::GotoOrOpenFileManager`).
2. Sets `current_dir = path.parent().unwrap_or(path)`.
3. Sets `selected_index` to the index of the entry whose path
   matches.

Existing call sites that will switch to this once it ships:
- Phase-7 search-by-name picker (Enter on a result).
- Phase-7 search-by-content picker (Enter — to the file's parent).
- Phase-8 symlink follow.

## Where it slots in

- `actions!` macro in
  [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs);
  derive-action variant since it carries a `PathBuf` payload.
- Register the handler in the FM's workspace observe hook.
- ~50 LOC.
