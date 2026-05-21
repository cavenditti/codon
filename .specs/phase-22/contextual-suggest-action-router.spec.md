---
id: TASK:phase-22/contextual-suggest-action-router
type: task
status: accepted
version: 0.1.0
summary: >
  Register the workspace action `codon_agent::ContextualSuggest`, wire
  the pane-router that resolves the focused pane kind to its legal
  response shapes, and surface the binding in the cheatsheet's Global
  section with a per-pane qualifier.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-contextual-suggest#c-global-action
  - REQ:codon/agent-contextual-suggest#c-pane-router
  - REQ:codon/agent-contextual-suggest#c-mode-bridge
  - REQ:codon/agent-contextual-suggest#c-no-selection-required
  - REQ:codon/agent-contextual-suggest#c-cheatsheet
aspects: [action-registration, pane-router, mode-bridge, no-selection, cheatsheet-entry]
---

# ContextualSuggest action + pane router

## Plan

- Add the `ContextualSuggest` action to
  [crates/codon-agent/src/actions.rs](spec:src:crates/codon-agent/src/actions.rs)
  alongside the existing `AgentExplain`/`AgentSummarize`/`AgentRefactor`.
- Register the workspace handler from `codon_agent::init`. The handler
  opens the input overlay (delivered by sibling task
  `contextual-suggest-input-overlay`).
- Implement `codon_agent::pane_router::resolve(window, cx) -> RouterDecision`
  where `RouterDecision { kind: PaneKind, allowed_shapes: &'static [ShapeId] }`.
  Read the focused entity via `CodonModeTracker` and the entity's
  `PaneModeBridge::kind()`. Single match expression, no callbacks.
- Add `"prefix '" = "codon_agent::ContextualSuggest"` to the embedded
  defaults in
  [crates/codon-keymap/src/keymap.rs](spec:src:crates/codon-keymap/src/keymap.rs)
  under the existing `# Agent (cmd-k a prefix)` block. Re-check `cmd-k '`
  is unbound first.
- The action MUST NOT require a selection — picks up whatever the
  preamble surfaces from `SelectionSource`. No selection-presence
  check in the handler.
- While the overlay is mounted the focused entity's bridge reports
  `PaneMode::Command` per the existing modal pattern (mirror
  [crates/codon-pickers/src/scaffold.rs](spec:src:crates/codon-pickers/src/scaffold.rs)).
- Surface the binding in the cheatsheet's Global section. The
  description column reads "Ask agent about this <kind>" with the
  kind chosen at render time so the user sees a contextual hint per
  pane.

## Acceptance

- `cmd-k '` from a terminal, editor, FM, and welcome page each opens
  the contextual-suggest overlay (no panic, no toast).
- `codon_agent::pane_router::resolve` has unit tests covering every
  pane kind that publishes a `PaneModeBridge` impl today.
- The cheatsheet (`cmd-k F1`) shows the binding under Global with a
  description that reflects the current pane kind.
- `vendor/zed/script/clippy` and `cargo test -p codon-agent` pass.
