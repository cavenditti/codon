---
id: TASK:phase-20/fm-hidden-rebind
type: task
status: draft
version: 0.0.1
summary: >
  Move the file-manager toggle-hidden binding from `.` to `, h`,
  joining the existing `,` view-options sub-prefix that already
  hosts the sort chords. Frees the bare `.` chord across every
  Normal-mode pane for the action-history repeat introduced by
  the sibling task.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/keymap-vocabulary#c-fm-hidden-rebind
blocked_by: []
---

# File-manager `.` → `, h`

## Plan

Today's user config
([`/Users/carlo/.config/codon/codon.toml`](spec:src:assets/config/codon.example.toml))
under `[bindings.file_manager.normal]` has the `,` view-options
prefix for sort chords:

```toml
", n" = "file_manager::SortByName"
", s" = "file_manager::SortBySize"
", m" = "file_manager::SortByMtime"
", c" = "file_manager::SortByBtime"
", e" = "file_manager::SortByExtension"
", r" = "file_manager::ToggleSortReverse"
```

The toggle-hidden chord today is `.` — bound in either the embedded
defaults' `[bindings.file_manager.normal]` block or directly in
[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
key handler. Phase 20 frees `.` for the action-history repeat
([TASK:phase-20/action-history-ring](spec:TASK:phase-20/action-history-ring)).

### What ships

1. **Locate** the current `.`-toggles-hidden binding. Check first
   `crates/codon-keymap/src/keymap.rs`'s `[bindings.file_manager.normal]`
   block, then the file-manager crate's raw key handler. There MUST
   be only one source of truth after this task — if both exist
   today, consolidate to the TOML defaults.

2. **Bind** `, h` to the toggle-hidden action under
   `[bindings.file_manager.normal]` in
   `crates/codon-keymap/src/keymap.rs`. Action name: whatever the
   existing toggle-hidden action is — search the file-manager crate
   for `ToggleHidden` or similar.

3. **Remove** the `.` binding from file-manager Normal mode. After
   this task, `.` in the file-manager produces a matcher dead-end
   (which `c-dead-end-flash` will surface as a status-bar flash)
   until the action-history-ring task lands and rebinds `.` to
   `codon_keymap::RepeatLast`.

4. **Update** the example config
   ([`assets/config/codon.example.toml`](spec:src:assets/config/codon.example.toml))
   to add the `, h = ToggleHidden` example next to the existing
   sort chords.

### Sequencing

This task is a hard prerequisite for
[TASK:phase-20/action-history-ring](spec:TASK:phase-20/action-history-ring)
because the ring task binds `.` to `RepeatLast` in the global
Normal predicate. Land this first so the file-manager doesn't
keep its private `.` binding shadow the global one.

## Acceptance

- `.` in the file-manager produces no action (until the
  action-history-ring task lands and rebinds it globally).
- `, h` in the file-manager toggles hidden files.
- `, n / s / m / c / e / r` continue to work for sorts.
- `spec lint` clean.

## Files touched

- `crates/codon-keymap/src/keymap.rs` — `[bindings.file_manager.normal]`
  edit.
- `crates/file-manager/src/file_manager.rs` (or wherever the
  current `.` handler lives) — remove the hard-coded binding.
- `assets/config/codon.example.toml` — example block.
