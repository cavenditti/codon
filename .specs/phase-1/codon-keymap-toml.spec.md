---
id: TASK:phase-1/codon-keymap-toml
type: task
status: accepted
version: 0.1.0
summary: >
  TOML keymap loader, full-screen cheatsheet modal on cmd-k F1, example
  user config, and 5-second chord timeout to accommodate multi-key chords.
owners: [carlo]
progress: done
refines:
  - REQ:codon/modal-shell#c-toml-keymap
---

# TOML keymap loader and cheatsheet

The `codon-keymap` crate parses `~/.config/codon/keymap.toml`, merges
it on top of an embedded default keymap, and binds the result via
`cx.bind_keys`. See [crates/codon-keymap/src/keymap.rs](spec:src:crates/codon-keymap/src/keymap.rs).

## What ships

- TOML loader supporting `[bindings.global]` plus per-pane
  `[bindings.{editor,terminal,file_manager}.{normal,insert}]` sections,
  resolved into Zed's `KeyBinding` value via an action-name match arm.
- `KeybindingsCheatsheetModal` (bound to `cmd-k F1`) that covers the
  workspace with a grouped, scrollable list of every binding reachable
  from the live dispatch tree. Chord rendering reuses
  `ui::KeyBinding::from_keystrokes` so each modifier and key shows as
  a platform-styled keycap. Codon-defined namespaces float to the top.
- `assets/config/keymap.example.toml` — a heavily-commented template
  documenting the section layout, keystroke syntax, and every default
  binding.
- A configurable keystroke-chord timeout: `gpui::set_keystroke_chord_timeout`
  exposes the previously-hardcoded 1-second value to embedders, and
  codon bumps it to 5 seconds during keymap load so slow typists can
  finish three-keystroke chords (`cmd-k s n`, `cmd-k shift-w n`, …)
  without the prefix flushing.
