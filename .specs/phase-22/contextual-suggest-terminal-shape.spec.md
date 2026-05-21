---
id: TASK:phase-22/contextual-suggest-terminal-shape
type: task
status: accepted
version: 0.1.0
summary: >
  Render `SuggestCommand` results inside the contextual-suggest
  overlay: pre-formatted command + one-line rationale, with Enter to
  prefill the PTY cursor (no auto-execute), `e` to edit in place
  before prefill, Esc to dismiss.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-contextual-suggest#c-terminal-command
  - REQ:codon/agent-contextual-suggest#c-no-auto-execute
aspects: [confirm-overlay, pty-prefill-no-newline]
---

# Terminal shape: SuggestCommand renderer

## Plan

- Extend `contextual_overlay.rs` (from sibling task) with a
  `CommandConfirm` view rendered when the harness returns
  `TurnOutcome::Suggestion(SuggestCommand { command, why })`.
- Layout: monospace block showing the command, a separator, the
  one-sentence rationale. Footer: `enter` prefill · `e` edit ·
  `esc` dismiss.
- Enter: route to the focused terminal pane. Call
  [`terminal_view::TerminalView`](spec:src:vendor/zed/crates/terminal_view/src/terminal_view.rs)
  with a "paste-as-input" helper that writes the literal command
  bytes to the PTY *without a trailing newline*. Focus returns to
  the terminal in Insert mode. Hard invariant: never append `\n`.
- `e`: swap the monospace block for an inline editor (Zed
  `Editor::multi_line`) seeded with the command. A second Enter
  prefills the (possibly-edited) text; Esc returns to the
  read-only view.
- Esc: close the overlay; nothing happens to the terminal.
- Add a thin `terminal_view` helper on the vendored side if one
  doesn't exist: `TerminalView::write_pending_command(&str)`.
  Public, no panic on absent terminal.

## Acceptance

- A synthetic harness run that returns
  `SuggestCommand { command: "ls -la", why: "..." }` against a
  terminal pane:
  - Enter writes the bytes `ls -la` to the PTY and the trailing
    newline is *not* present (verified by an integration test that
    captures PTY writes).
  - `e` opens the inline editor; Enter writes the edited bytes.
  - Esc closes without writing to the PTY.
- Pasting in editor or FM panes is a no-op + harness trace marks the
  shape as illegal (router enforced by the harness/pane-tools tasks).
- `vendor/zed/script/clippy` clean.
