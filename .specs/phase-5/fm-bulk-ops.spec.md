---
id: TASK:phase-5/fm-bulk-ops
type: task
status: accepted
version: 0.0.1
summary: >
  Bulk rename / delete on marked file-manager entries. Operates on
  the existing mark set established with `v`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-enhancements#c-bulk-ops
---

# Bulk operations on marked files

## What ships

With one or more entries marked (via `v`):

- `D` (shift-d) — delete every marked file (with single confirm prompt)
- `R` (shift-r) — bulk rename. Opens the input bar with a pattern; the
  pattern uses `{}` as a counter placeholder, e.g. `screenshot-{}.png`

Single-entry `d` / `r` keep their existing semantics — bulk uses the
capitalised variants.

## Where it slots in

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  already maintains `marked: BTreeSet<usize>` and renders marked rows
  with a full-line highlight. The single-entry delete / rename
  handlers exist (search `handle_delete` / `handle_rename`); duplicate
  them with the marked set iteration.

## Approach

Tight follow-up to
[TASK:phase-5/fm-copy-paste](spec:TASK:phase-5/fm-copy-paste). Both
tasks edit the same key dispatch; this one lands second so it can
reuse the same input-bar pattern (and the same async fs hooks). ~60
LOC of mostly mechanical code.
