---
id: TASK:phase-14/modals-extract-scaffold
type: task
status: accepted
version: 0.0.1
summary: >
  Extract a single shared `ModalScaffold` builder in `codon-pickers`
  that owns the `FocusHandle`, the `Focusable` impl, the
  `EventEmitter<DismissEvent>` boilerplate, and the
  `CodonModeTracker.command_active` toggle; migrate the five existing
  codon modals onto it by composition.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-modal-scaffolding-shared
---

# Extract `ModalScaffold` and migrate five callsites

## What changes

`codon-pickers` grows a tiny `scaffold` module with a
`ModalScaffold` struct + `ModalModeTag` enum. The builder owns the
parts of every codon modal that are currently duplicated:

- a `FocusHandle` allocated up-front,
- a hook for toggling the global `CodonModeTracker.command_active`
  flag (only when the modal is the command palette — every other
  codon modal leaves the indicator alone).

Each migrated modal holds the scaffold by composition (`scaffold:
ModalScaffold`) and forwards `Focusable::focus_handle` to it. The
`impl EventEmitter<DismissEvent>` stays per-struct — it's a trivial
zero-body impl that doesn't benefit from sharing.

The five callsites:

- `crates/codon-keymap/src/cheatsheet_modal.rs` — `ModalModeTag::Inert`.
- `crates/codon-command-palette/src/modal.rs` — `ModalModeTag::CommandActive`;
  the inline `set_command_active` helper and the inline
  `cx.update_global::<CodonModeTracker, _>` call are removed.
- `crates/codon-session/src/picker.rs` — `Inert`.
- `crates/codon-session/src/window_picker.rs` — `Inert`.
- `crates/codon-pickers/src/dir_picker.rs` — `Inert`.

The session-overview modal is intentionally out of scope: its
layout-swap logic intersects with focus management in ways that need
a separate refactor.

## Approach

1. Add `codon-mode.workspace = true` to `crates/codon-pickers/Cargo.toml`.
2. Create `crates/codon-pickers/src/scaffold.rs` exposing
   `ModalScaffold` + `ModalModeTag` + a single unit test that
   exercises both tag variants.
3. Re-export from `codon_pickers.rs`.
4. Migrate each callsite one at a time, running `cargo check -p
   <crate>` between steps. After all five: `cargo build -p codon`
   end-to-end.
5. `rg -n 'update_global::<CodonModeTracker' crates/` must show zero
   hits outside `crates/codon-pickers/src/scaffold.rs`.

## Non-goals

- No new `codon-ui` crate. Composition over inheritance keeps this
  to a single small module in an existing crate.
- No change to `crates/codon-session/src/overview.rs` (the session
  overview modal). That modal's layout-swap dance needs a separate
  TASK.
- No drive-by `unwrap()` / `let _ =` fixes — those are tracked
  separately.

## Files touched

- `.specs/codon/code-quality.spec.md` — add the
  `#c-modal-scaffolding-shared` clause.
- `crates/codon-pickers/Cargo.toml` — add `codon-mode` dep.
- `crates/codon-pickers/src/scaffold.rs` — new module.
- `crates/codon-pickers/src/codon_pickers.rs` — re-export.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — migrate.
- `crates/codon-command-palette/src/modal.rs` — migrate, drop the
  inline `set_command_active`.
- `crates/codon-session/src/picker.rs` — migrate.
- `crates/codon-session/src/window_picker.rs` — migrate.
- `crates/codon-pickers/src/dir_picker.rs` — migrate.

## Verification

- `cargo build -p codon` is clean.
- `cargo clippy -p codon-pickers -p codon-keymap -p codon-command-palette
  -p codon-session --no-deps -- -D warnings` passes.
- `cargo test -p codon-pickers` passes, including a new scaffold
  unit test.
- The command-palette status indicator still shows `CMD` while the
  palette is open and reverts when it closes.
