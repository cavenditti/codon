---
id: TASK:phase-5/command-palette-keyboard-parity
type: task
status: accepted
version: 0.0.1
summary: >
  Every palette interaction is bound to a keystroke — cycle, jump to
  argument mode, run, dismiss. No mouse-only paths.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-palette#c-keyboard-parity
---

# Keyboard-only parity

## What ships

A short, opinionated keymap inside the codon command palette modal
context (`CommandPalette > Picker`):

| Key | Action |
|---|---|
| `up` / `ctrl-p` | previous row |
| `down` / `ctrl-n` | next row |
| `space` | (in Command mode, when a completer exists) jump to Argument mode |
| `tab` | accept completion partial (Argument mode only) — fills the row's `value` into the query without dispatching |
| `enter` | confirm the active row |
| `esc` | Argument → Command, or Command → close |

These bindings live in the embedded codon default keymap so they
ship with the binary; users can override per the existing
`[bindings.*.normal]` TOML scheme.

The description pane is read-only — no clickable controls. The
chord rendered there is for documentation; running a command via
its chord uses the normal dispatch path, not a palette-internal
shortcut.

## Reference points

- [`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
  — codon's embedded default keymap. The new bindings live in a
  `[bindings.command_palette]` table parallel to
  `[bindings.file_manager.normal]`.
- [`vendor/zed/crates/picker/src/picker.rs`](spec:src:vendor/zed/crates/picker/src/picker.rs)
  — existing picker bindings (`SelectNext`, `SelectPrevious`,
  `Confirm`, `Cancel`) — codon's are aliases.

## Tests

- Manual: open palette, drive every interaction listed above without
  touching the trackpad. Confirm each works.

Effort: low. ~50 LOC of bindings + a small `Picker` config layer.
