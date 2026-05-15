---
id: TASK:phase-14/modals-extract-scaffold
type: task
status: draft
version: 0.0.1
summary: >
  Extract a single `ModalScaffold` builder in `codon-pickers` that
  wraps the focus_handle + EventEmitter<DismissEvent> +
  `set_command_active` triplet, and migrate the five existing
  codon modals/pickers (cheatsheet, command-palette, session
  picker, window picker, dir picker) onto it.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-modal-scaffolding-shared
---

# Shared modal scaffold for every codon modal/picker

## What changes

Today five codon modals/pickers each re-implement the same three
moving parts:

- A `focus_handle: FocusHandle` field + `Focusable` impl.
- `impl EventEmitter<DismissEvent>` + a `Modal::on_blur_close` hook.
- A `cx.update_global::<CodonModeTracker, _>(|t, _| t.set_command_active(...))`
  toggle on open/dismiss (only some crates do this; the inconsistency
  is the second bug under this clause).

The five callsites:

- `crates/codon-keymap/src/cheatsheet_modal.rs` (`CheatsheetModal::new`).
- `crates/codon-command-palette/src/modal.rs` (`CommandPaletteModal::new`,
  toggles `command_active` at line 40).
- `crates/codon-session/src/picker.rs` (`SessionPicker::new`).
- `crates/codon-session/src/window_picker.rs` (`WindowPicker::new`).
- `crates/codon-pickers/src/dir_picker.rs` (`DirPicker::new`).

A sixth (`crates/codon-session/src/overview.rs`) also has the shape
but its lifecycle is more involved (window preview, layout swap);
adoption there is optional in this TASK and can move to a follow-up
if the scaffold doesn't fit cleanly.

## Approach

1. Add `ModalScaffold` to `crates/codon-pickers/src/lib.rs` (or a new
   `crates/codon-pickers/src/scaffold.rs` module). API sketch:

   ```rust
   pub struct ModalScaffold {
       pub focus_handle: FocusHandle,
       pub mode_tag: ModalModeTag,
   }

   pub enum ModalModeTag {
       /// Sets CodonModeTracker.command_active = true while open.
       CommandActive,
       /// Does not touch the mode tracker — modal is presentational.
       Inert,
   }

   impl ModalScaffold {
       pub fn new(cx: &mut Context<impl Any>, tag: ModalModeTag) -> Self { ... }
       pub fn on_dismiss(&self, cx: &mut App) { ... }
   }
   ```

   The scaffold reuses `gpui::FocusHandle::new(cx)`, `cx.focus_handle()`,
   `DismissEvent`, and `workspace::ModalView` — it does NOT re-export
   those, it composes over them.

2. Add a `derive`-like helper macro in the same module:
   `codon_modal!(MyModal, CommandActive)` that emits:
   - the `focus_handle` field on `MyModal`
   - `Focusable` impl
   - `EventEmitter<DismissEvent>` impl
   - the dismiss hook wired to `ModalScaffold::on_dismiss`

   If the macro turns out to be more abstraction than reuse warrants,
   skip it and use the plain struct + a `pub fn install_dismiss_hook`
   helper. Don't over-engineer.

3. Migrate the five callsites listed above. Each migration is one
   commit. The diff per callsite should be:
   - Remove the local `focus_handle: FocusHandle` field; replace with
     `scaffold: ModalScaffold` (which holds the focus handle).
   - Remove the inline `Focusable` and `EventEmitter<DismissEvent>` impls
     unless the macro is in use.
   - Replace any direct `set_command_active` calls with the scaffold's
     lifecycle hooks.

4. The command-palette is the most opinionated callsite — it currently
   toggles `command_active` from `modal.rs:40`. The scaffold's
   `ModalModeTag::CommandActive` variant replaces that line.

## Non-goals

- Not touching the underlying `picker::Picker` from vendored Zed.
  `Picker` already encodes the search-input + result-list + delegate
  shape; the scaffold sits above it for the modal-shell concerns.
- Not migrating `overview.rs` in this TASK unless it's a one-line
  swap. Its layout-swap logic is out of scope.
- Not introducing a new `codon-ui` crate. `codon-pickers` already
  hosts shared modal-adjacent code; reuse it.

## Files touched

- `crates/codon-pickers/src/lib.rs` (or new `scaffold.rs`) — new
  `ModalScaffold` and (optional) helper macro.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — adopt scaffold.
- `crates/codon-command-palette/src/modal.rs` — adopt scaffold;
  remove inline `cx.update_global` at line 40.
- `crates/codon-session/src/picker.rs` — adopt scaffold; move the
  `_assert_actions` dummy to a `#[cfg(test)]` module in the same
  file (overlaps with `dead-code-purge` TASK — coordinate).
- `crates/codon-session/src/window_picker.rs` — adopt scaffold.
- `crates/codon-pickers/src/dir_picker.rs` — adopt scaffold.
- `crates/codon-pickers/Cargo.toml` — add `codon-mode` dep so the
  scaffold can touch the tracker (this is the only cross-crate dep
  added; it does not regress `#c-keymap-decoupled` because
  `codon-pickers` is not `codon-keymap`).

## Verification

- `cargo build -p codon-pickers -p codon-keymap -p codon-command-palette -p codon-session` — clean.
- `cargo test -p codon-pickers` — passes (add at least one test that
  constructs a `ModalScaffold` with each `ModalModeTag` variant and
  asserts the tracker state transitions).
- Manual smoke: open cheatsheet (`cmd-k F1`), command palette,
  session picker, window picker, dir picker. Each opens, dismisses
  on Esc, restores focus on close. The mode indicator in the status
  bar reads `CMD` while the command palette is open, and does NOT
  read `CMD` while the cheatsheet is open (cheatsheet uses `Inert`).
- `rg -n 'cx\.update_global::<CodonModeTracker' crates/` returns hits
  only inside the scaffold module itself; no direct callsites
  elsewhere.
