---
id: TASK:phase-11/window-direct-index
type: task
status: accepted
version: 0.1.0
summary: >
  Bind `WindowGoto(0..=8)` to digit keys 1–9 (tmux `prefix 0-9`).
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-direct-index
categorized_under: [TOPIC:topics/phase-11]
---

# Direct index goto bindings

## What ships

- Default keymap binds `cmd-k 1` … `cmd-k 9` to
  `codon_session::WindowGoto(0)` … `WindowGoto(8)`.
- The rebinder in `crates/codon-keymap/src/keymap.rs` already routes
  parameterized actions through the `Action::build` path; verify the
  TOML form `"codon_session::WindowGoto(0)"` parses and rebinds.
  If today's parser only handles unit-struct action names, extend
  the match arm so digit→index parses cleanly.
- 1-based numbering on the keymap, 0-based in the action — matches
  tmux's convention without forcing the action's internal type to
  change.
- Out-of-range indices (e.g. user has 3 windows but presses `cmd-k 7`)
  are no-ops with a `log::debug!` line, never a panic. The existing
  `handle_window_goto` is already permissive on this.

## Why this shape

`WindowGoto(usize)` was declared a Derive(Action) struct back in
phase-2 specifically so the keymap could pass an index. Phase-2
shipped the action; this task closes the loop by actually wiring
keys to it.

Effort: small. Mostly keymap text + (potentially) a parser tweak.
