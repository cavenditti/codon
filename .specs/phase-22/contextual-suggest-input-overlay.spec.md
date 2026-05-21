---
id: TASK:phase-22/contextual-suggest-input-overlay
type: task
status: accepted
version: 0.1.0
summary: >
  Build the NL input modal that opens on `ContextualSuggest`: a single-
  line editor over `codon-pickers::ModalScaffold` with a per-pane
  prompt label, Enter to submit, Escape to cancel (also cancels an
  in-flight turn).
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-contextual-suggest#c-input-overlay
  - REQ:codon/agent-contextual-suggest#c-cancellation
aspects: [input-modal, cancel-wiring]
---

# Contextual-suggest input overlay

## Plan

- New module `crates/codon-agent/src/contextual_overlay.rs` exposing
  `ContextualOverlay::open(window, cx, kind, harness)`. Built on
  [`codon-pickers::ModalScaffold`](spec:src:crates/codon-pickers/src/scaffold.rs).
- Layout: a single-line editor (reuse Zed's `Editor::single_line`),
  a prompt label whose text comes from `kind.input_label()`
  (`"ask about this terminal"`, `"ask about this buffer"`,
  `"ask about this directory"`, `"ask"`), and a footer hint
  showing Enter / Esc bindings.
- On Enter: capture the input string, call
  `harness.run_turn(preamble, user_msg, tools, cancel_token)`. The
  preamble + tools come from sibling tasks; this task wires the
  call site.
- While a turn is in flight: replace the footer hint with a spinner
  + "Esc to cancel". Esc sets the cancel token; the harness returns
  `TurnOutcome::Cancelled`; the overlay closes.
- On `TurnOutcome::Suggestion(shape)`: hand off to the shape-
  specific renderer (terminal-command / editor-action / fm-action /
  response, delivered by sibling tasks). On `TurnOutcome::Cancelled`
  or `Error`: close silently (errors land in the harness trace).

## Acceptance

- Opening the overlay from any pane shows the contextual prompt
  label.
- Esc during the input phase closes the overlay without calling the
  harness.
- Esc during an in-flight turn fires the cancel token. The turn's
  trace entry shows `outcome = cancelled`.
- The status bar mode pill shows `Command` while the overlay is open
  and returns to the previous mode on dismiss.
- `cargo test -p codon-agent` passes.
