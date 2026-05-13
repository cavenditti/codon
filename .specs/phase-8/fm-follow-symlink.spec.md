---
id: TASK:phase-8/fm-follow-symlink
type: task
status: accepted
version: 0.0.1
summary: >
  Enter on a symlinked dir follows the link. `F` (shift-f) on any
  symlinked entry resolves and reveals via codon_fm::Reveal.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-symlinks#c-follow-symlink
---

# File-manager follow symlink

## What ships

- The existing `enter_directory` (file_manager.rs ~line 361)
  already follows symlinks implicitly via `std::fs::metadata`
  which resolves links. Re-verify and document this is the
  intended behavior.
- New chord `F` (shift-f) on any entry: `std::fs::read_link` →
  if Some, dispatch `codon_fm::Reveal(target)` (relies on
  TASK:phase-6/fm-reveal-action).
- Both paths share a `resolve_with_depth_cap` helper that caps
  link traversal at 16 to defend against pathological symlink
  loops.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
`enter_directory` (verification) + new `F` dispatch arm. ~70 LOC.
