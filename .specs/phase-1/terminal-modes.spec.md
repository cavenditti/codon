---
id: TASK:phase-1/terminal-modes
type: task
status: accepted
version: 0.1.0
summary: >
  Terminal pane respects PaneMode (Normal scrollback / Insert PTY) and
  ignores chord-prefix replays in Normal mode.
owners: [carlo]
progress: done
refines:
  - REQ:codon/modal-shell#c-pane-mode
---

# Terminal pane modes

`TerminalView` carries its own `pane_mode` field separate from Zed's
Vim mode. Default is `Insert` (raw keystrokes go to the PTY).
Double-escape within 300ms enters `Normal`, exposing scrollback
navigation (j/k for line, ctrl-u/d for half-page, gg/G for top/bottom).
`i` or `a` returns to Insert.

## Chord-prefix safety

`handle_normal_key` rejects keystrokes carrying `platform` or `alt`
modifiers, so when GPUI replays a chord prefix (e.g. `cmd-k` waiting
for a continuation that never came) the terminal stays cleanly in
Normal mode instead of interpreting the `k` as a bare scroll binding.
See [terminal_view::handle_normal_key](spec:src:vendor/zed/crates/terminal_view/src/terminal_view.rs).
