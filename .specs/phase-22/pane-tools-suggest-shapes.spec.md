---
id: TASK:phase-22/pane-tools-suggest-shapes
type: task
status: accepted
version: 0.1.0
summary: >
  Implement the three reply-shaping tools — `suggest_command`,
  `suggest_action`, `suggest_response` — and gate them by the pane
  router. Illegal shapes return a structured tool error so the model
  can retry.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-pane-tools#c-tool-suggest-command
  - REQ:codon/agent-pane-tools#c-tool-suggest-action
  - REQ:codon/agent-pane-tools#c-tool-suggest-response
  - REQ:codon/agent-pane-tools#c-router-enforcement
aspects: [suggest-command, suggest-action, suggest-response, router-gate]
---

# Tools: SuggestCommand / SuggestAction / SuggestResponse

## Plan

- New module `crates/codon-agent/src/tools/suggest.rs`.
- Types:
  - `SuggestCommand { command: String, why: String }` — invariant:
    `command` does not end with `\n`. Server-side trim with a
    trace warning if present.
  - `SuggestAction { action_name: String, payload: serde_json::
    Value, why: String }`.
  - `SuggestResponse { text: String }`.
- Tool handlers DO NOT render the overlay themselves. They convert
  the args into the matching `TurnOutcome::Suggestion(...)` variant
  and return; the harness ends the turn and the overlay (from
  sibling contextual-suggest tasks) renders.
- Router enforcement: each handler consults
  `codon_agent::pane_router::resolve(...)`. If the shape isn't in
  `allowed_shapes`, return tool error
  `shape_illegal_for_pane { active_kind, allowed_shapes,
  attempted_shape }`. The harness surfaces this back to the model
  as the tool result so it can retry with a legal shape (no turn
  failure).
- `SuggestAction::action_name` is validated against Zed's global
  action registry at tool-call time. An unresolved name returns
  `action_unknown { name }`.

## Acceptance

- A synthetic turn calling `suggest_command { command: "ls" }`
  against an editor returns `shape_illegal_for_pane` and lets the
  model retry with `suggest_action`.
- A `suggest_action` with `action_name: "not::A::Real::Action"`
  returns `action_unknown`.
- `suggest_command` whose `command` ends in `\n` is accepted, the
  trailing newline is stripped, and the harness trace contains a
  `command_trim_newline` warning.
- `cargo test -p codon-agent` passes.
