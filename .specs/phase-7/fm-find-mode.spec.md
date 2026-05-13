---
id: TASK:phase-7/fm-find-mode
type: task
status: accepted
version: 0.0.1
summary: >
  `/` enters find-forward, `?` find-backward; `n` / `N` walk matches
  after commit. Substring case-insensitive match against entry names.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-find-search#c-find-mode
---

# File-manager find / `/` and `?`

## What ships

Two new `PendingInput` variants — `FindForward { query, last_pattern }`
and `FindBackward { query, last_pattern }`. Each behaves like
yazi find:

- On every character typed, jump `selected_index` to the next /
  previous entry containing the substring (case-insensitive).
- `Enter` commits: store the typed query as `last_find_pattern: Option<String>`
  on `FileManager`; leave Insert mode.
- `Esc` cancels; nothing changes.

`n` / `N` (in Normal mode) walk forward / backward through matches
of `last_find_pattern`. No-op if no pattern is committed.

## Migration note

Pre-phase-7, codon's `/` is filter (hide non-matches). That
behavior moves to `f` — see TASK:phase-7/fm-filter-rebind. Both
specs land in the same PR to keep the keymap coherent.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs):
extend `PendingInput` + `handle_insert_key`; add `n` / `N`
dispatch arms in Normal. ~160 LOC.
