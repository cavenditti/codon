---
id: TASK:phase-7/fm-filter-rebind
type: task
status: accepted
version: 0.0.1
summary: >
  Migrate codon's existing `/`-as-filter behavior to `f`. Frees `/` for
  find-forward (see TASK:phase-7/fm-find-mode).
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-find-search#c-filter-rebind
---

# File-manager filter rebind to `f`

## What ships

The existing fuzzy-filter behavior (Insert-mode prompt, hide
non-matching entries, Esc clears) moves from `/` to `f` verbatim.
No semantic change — just the chord.

Cheatsheet text and any leftover doc comments that reference `/`
as filter get updated in the same PR.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs):
single dispatch-arm change. Pair with TASK:phase-7/fm-find-mode in
one PR. ~20 LOC.
