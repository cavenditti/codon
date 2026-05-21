---
id: TASK:phase-22/memory-picker
type: task
status: accepted
version: 0.1.0
summary: >
  Add the `codon_memory::MemoryPicker` modal bound to `prefix m`:
  fuzzy-searchable list of every memory in the current workspace's
  store, with keys to open / pin / delete / create.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shared-memory#c-picker
---

# Memory picker

## Plan

- New module `crates/codon-memory/src/picker.rs` built on
  [`codon-pickers::ModalScaffold`](spec:src:crates/codon-pickers/src/scaffold.rs)
  and Zed's `picker::Picker`.
- Item rendering: `[pin?]  title  tags  (created)`. Pinned items
  float above the rest; within each group, sort by `created`
  descending.
- Bindings (inside the picker):
  - `enter` — open the memory file in an editor pane (read-write
    on disk; the picker doesn't lock).
  - `p` — toggle pinned. Updates the file frontmatter atomically.
  - `dd` — delete with a confirm prompt ("delete <title>? y/n").
  - `c` — create a new memory by opening an empty `*.md` file
    seeded with the frontmatter template.
- Add `"prefix m" = "codon_memory::OpenPicker"` to embedded defaults
  in
  [crates/codon-keymap/src/keymap.rs](spec:src:crates/codon-keymap/src/keymap.rs).
  Check `cmd-k m` isn't already bound first; if it is, surface the
  collision in this task's PR and re-pick.
- Soft-warning row: when `MemoryStore::warn_if_oversize` returns
  `Some(n)`, render a banner at the top of the picker:
  "Store is <n> KiB — consider pruning."

## Acceptance

- `cmd-k m` from any pane opens the picker.
- The picker lists every memory in the current workspace's store.
- `p` toggles pinned and persists across reopen.
- `dd` with confirm removes the file from disk.
- `c` creates a new file the user can edit and save.
- `cargo test -p codon-memory` passes.
