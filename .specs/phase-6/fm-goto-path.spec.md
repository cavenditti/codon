---
id: TASK:phase-6/fm-goto-path
type: task
status: accepted
version: 0.0.1
summary: >
  `:cd <path>` prompt — opens an Insert-mode input bar that accepts
  absolute, relative, and `~`-prefixed paths with filesystem
  tab-completion.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-nav-extras#c-goto-path
---

# File-manager goto-path

## What ships

`:cd` from inside the FM Normal mode opens the existing
Insert-mode input bar with a new `PendingInput::GotoPath { query:
String }` variant. Each `Tab` keystroke extends the query with
the longest-common-prefix of matching candidates from the
filesystem; Enter sets `current_dir`. `~` expands to
`$HOME`; relative paths resolve against the active `current_dir`.

Failure modes (path doesn't exist, isn't a dir, no read perm) all
surface via the existing `surface_error` toast.

## Where it slots in

- Add `PendingInput::GotoPath` to the enum in
  [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs).
- The `:` chord already opens the codon command palette in FM
  Normal mode; reroute `:cd <path>` through the palette as a
  built-in command rather than a new top-level chord. Read
  `crates/codon-command-palette/` for the existing built-in
  completer pattern.
- Completion candidates come from `std::fs::read_dir` on the
  longest existing prefix of the typed string. ~120 LOC.
