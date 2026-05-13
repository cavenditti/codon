---
id: TASK:phase-6/fm-sort-reverse
type: task
status: accepted
version: 0.0.1
summary: >
  `,,` toggles the sort direction. Persisted with the chosen sort
  mode.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-sort-display#c-sort-reverse
---

# File-manager reverse sort

## What ships

A `reverse: bool` field on `FileManager`. `,,` (the `,` chord
followed by another `,`) toggles it. The comparator wraps its
`Ordering` in `if reverse { o.reverse() } else { o }`.

Directories-first ordering is preserved regardless of reverse —
reverse only flips the *within-group* ordering, not the
dirs-before-files invariant.

## Where it slots in

Same dispatch chord as TASK:phase-6/fm-sort-modes; this lands as
an additive arm. ~30 LOC.
