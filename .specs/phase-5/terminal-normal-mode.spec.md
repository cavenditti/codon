---
id: TASK:phase-5/terminal-normal-mode
type: task
status: accepted
version: 0.0.1
summary: >
  Terminal panes gain a `PaneMode::Normal` state for scrollback /
  selection editing. Double-Esc toggles in and out; alacritty's vi
  mode powers cursor motion, selection, and yank.
owners: [carlo]
progress: done
refines:
  - REQ:codon/modal-shell#c-terminal-normal-mode
---

# Terminal Normal mode

## What ships

Behaviour in a terminal pane:

- **Double-Esc (≤ 300 ms)** toggles `PaneMode::Insert ⇄ Normal`. The
  chord is detected in both the `key_down` listener and the
  `SendKeystroke` action handler — default macOS keymap routes raw
  Esc through `SendKeystroke`, and GPUI stops propagation in the
  action's bubble phase before `key_down` would fire, so the
  detection has to live in both entry points.
- **Normal mode** enables alacritty's vi mode under the hood
  (`Terminal::toggle_vi_mode`). `try_keystroke` then routes h/j/k/l,
  w/b/e, $/0/^, H/M/L, `%`, g/G, ctrl-u/ctrl-d/ctrl-b/ctrl-f for
  motion + scrolling; `v` starts a selection, `y` yanks to clipboard,
  `escape` clears the selection.
- **Codon-owned keys in Normal mode**: `:` opens the command palette,
  `i` / `a` exit back to Insert (also syncs vi_mode off + scrolls to
  bottom). Every other Normal-mode key is consumed via
  `cx.stop_propagation` so unbound keys can't sneak through to the
  PTY via the text-input handler.
- **PTY write gate**: `commit_text` and `set_marked_text` early-return
  when `pane_mode == Normal`. The IME / typed-text path is the only
  way unbound keys would otherwise reach the shell, so the gate has
  to live at that boundary too.

## Why this shape

Alacritty's vi mode is already battle-tested for terminal-scrollback
navigation and selection — re-implementing motions in codon would
duplicate that work and drift over time. Codon owns only the bits
alacritty doesn't: pane-mode bookkeeping for the status bar / key
context, the double-Esc chord, and `:`-palette dispatch.

## Files touched

- `vendor/zed/crates/terminal_view/src/terminal_view.rs` —
  `key_down`, `send_keystroke`, `handle_normal_key`,
  `handle_double_escape`, `enter_normal_mode`, `exit_normal_mode`,
  `commit_text`, `set_marked_text`.

## Known gaps (deferred)

- The terminal renders alacritty's cursor position, not the vi
  cursor, so the visible block doesn't move in Normal mode until the
  user starts a selection. Fixing that is a vi-cursor overlay in the
  terminal element — separate work.
