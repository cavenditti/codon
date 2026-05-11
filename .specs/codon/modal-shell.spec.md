---
id: REQ:codon/modal-shell
type: requirement
status: accepted
version: 1.0.0
level: MUST
summary: >
  Always-modal shell (PaneMode Normal/Insert/Command) with codon-keymap
  TOML loader, mode indicator in the status bar, and Helix mode forced
  on by default.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-1]
---

# Modal shell

## Context

Codon is built on Zed but treats Helix mode as the default editing
model. Every pane has a `PaneMode` — terminals are Insert by default,
file manager is Normal, editor delegates to Vim/Helix. Mode is shown
in the status bar; the `:` key opens the command palette in Normal
mode regardless of pane type.

:::{requirement id="modal-shell" level="MUST"}
The system MUST provide:

- {#c-pane-mode} a `PaneMode` enum (Normal / Insert / Command) tracked
  globally in `CodonModeTracker`
- {#c-helix-default} Helix mode force-enabled by default in vim
- {#c-mode-indicator} a status bar indicator showing the active mode
- {#c-toml-keymap} a TOML keymap loader at
  `~/.config/codon/keymap.toml` overriding the embedded defaults
- {#c-terminal-normal-mode} a `PaneMode::Normal` state for terminal
  panes entered via double-Esc that disables PTY writes, enables
  alacritty's vi mode for cursor motion / selection / yank, opens the
  command palette on `:`, and returns to Insert mode on `i`/`a` or a
  second double-Esc
- {#c-mode-indicator-themable} the mode indicator MUST be prominent
  enough to read at a glance (bold pill / colored background, not just
  inline text) and MUST honour the active theme — colors come from
  theme tokens, not hard-coded hex
:::
