---
id: TASK:phase-7/fm-zoxide-jump
type: task
status: accepted
version: 0.0.1
summary: >
  `z` opens a zoxide-backed picker. Enter sets current_dir. No-op +
  toast when zoxide is missing.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-find-search#c-zoxide-jump
---

# File-manager zoxide jump

## What ships

`z` opens a `Picker` over the output of `zoxide query -l` (highest
scored first). Type-as-you-go fuzzy filter further narrows the
list. Enter sets `current_dir` and clears the back-stack
(forward-stack semantics: this is a forward navigation).

No `zoxide` binary → toast and dismiss.

## Approach

- One-shot subprocess via `std::process::Command`. zoxide's `-l`
  output is line-delimited paths, sorted descending by score.
- Reuse the picker shape from TASK:phase-7/fm-search-by-name.

~80 LOC.
