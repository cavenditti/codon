---
id: TASK:phase-6/fm-history-stack
type: task
status: accepted
version: 0.0.1
summary: >
  Directory-history stack for the file manager. `[` / `ctrl-o` step
  back; `]` / `ctrl-i` step forward.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-nav-extras#c-history-back-forward
---

# File-manager directory history

## What ships

A bounded stack of recent `current_dir` values on `FileManager`,
managed like a browser's back/forward:

- Every successful `set_current_dir` push onto the back-stack
  (skipping no-op re-navigations).
- `[` / `ctrl-o` pops one from the back-stack, pushes onto the
  forward-stack, and sets `current_dir`.
- `]` / `ctrl-i` does the inverse.
- A "forward" navigation (Enter into a dir, `:cd`) clears the
  forward-stack — same browser semantics.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
— add `back_stack: VecDeque<PathBuf>` + `forward_stack:
VecDeque<PathBuf>` fields (cap each at 64). Wire `HistoryBack` /
`HistoryForward` actions into the existing Normal-mode dispatch.

~80 LOC.
