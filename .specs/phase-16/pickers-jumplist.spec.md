---
id: TASK:phase-16/pickers-jumplist
type: task
status: draft
version: 0.0.1
summary: >
  Implement `codon_pickers::JumplistPicker` — a
  `picker::Picker` delegate over the active editor's vim jumplist
  joined with codon-session's pane-activation history. Bound to
  `prefix p j`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/helix-pickers#c-jumplist-picker
---

# Jumplist picker

## What changes

The picker shows two layered concepts under one fuzzy-matched list:

1. **Vim jumplist entries** for the active editor. Zed's vim crate
   already maintains a per-editor jumplist via the `JumpList` type
   ([`vendor/zed/crates/vim/src/`](spec:src:vendor/zed/crates/vim/));
   it's read internally by `Ctrl-i` / `Ctrl-o`. Expose it through
   a small public accessor (`Vim::jumplist_entries(&self) -> &[Jump]`
   or similar — additive vendored-Zed surface following
   [`workspace::codon_bridge`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs)
   conventions).
2. **Codon pane-activation history.** `codon-session`'s
   `WindowRuntimeCache` tracks recent panes for window-switching;
   stash the most recent ~20 pane activations in a session-scoped
   ring buffer keyed `(window_id, pane_id, item_id_if_any)`.

Picker rows render as:

```
[jump] src/main.rs:42 — fn run()
[jump] crates/codon-session/src/runtime.rs:118 — impl WindowRuntimeCache
[pane] Terminal — ~/Devel/personal/codon_v3
[pane] FileManager — ~/Devel/personal/codon_v3/crates
```

Confirming a row:

- **`[jump]`** — activates the editor + scrolls to the recorded
  anchor (same as `Ctrl-i`/`Ctrl-o` semantics).
- **`[pane]`** — activates the pane via
  `codon_session::WindowSwitch`-style focus, restoring the cached
  `Member` tree if needed.

New crate or new module under existing crate:

- Prefer **module**: add `crates/codon-pickers/src/jumplist.rs`
  (the `codon-pickers` crate already exists as the shared
  ModalScaffold home).
- Action: `codon_pickers::JumplistPicker` registered via
  `actions!(codon_pickers, [JumplistPicker])`.

Binding (added to
[`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)):

```toml
"prefix p j" = "codon_pickers::JumplistPicker"
```

The picker uses `picker::Picker` (no custom UI; standard codon
picker shell). `PickerDelegate::confirm` dispatches the appropriate
focus / scroll action.

## Why this clause

Helix's `space j` is the one space-mode picker that has no Zed
analog. It's also more useful in codon than in Helix because codon
maintains *pane* history that Helix doesn't have — surfacing both
layers in one picker is the natural codon-shaped extension.

## Verification

- Open codon in a project. Make jumps in an editor (g d, g f,
  etc.). Press `cmd-k p j`. The jumplist picker shows the recorded
  jumps.
- Switch to a terminal pane, then a file manager, then back. The
  jumplist picker now includes those pane activations.
- Confirm a `[jump]` row → editor focuses and scrolls.
- Confirm a `[pane]` row → pane activates.
- Cheatsheet renders `JumplistPicker` under its global tab.

## Done when

- The picker action is registered and bound.
- Both layers (jumplist + pane history) appear in the list.
- Confirm semantics match the above.
- Add at least one integration test under
  `crates/codon-pickers/src/tests.rs` exercising "populate
  history, open picker, assert rows" — the focus / scroll side
  can stay UI-tested manually.
- `spec lint` is at zero errors.
