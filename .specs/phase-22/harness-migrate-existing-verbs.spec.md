---
id: TASK:phase-22/harness-migrate-existing-verbs
type: task
status: accepted
version: 0.1.0
summary: >
  Migrate `AgentExplain` / `AgentSummarize` / `AgentRefactor` to call
  `codon_agent::Harness::run_turn` so they share the preamble,
  cancellation, and trace surface with contextual-suggest. The
  selection-seeded user-facing behaviour is preserved.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-harness#c-existing-verbs-migrated
blocked_by:
  - TASK:phase-22/harness-api
---

# Migrate cross-pane verbs onto the harness

## Plan

- The three verbs today live in
  [crates/codon-agent/src/actions.rs](spec:src:crates/codon-agent/src/actions.rs)
  and bypass the harness by calling
  `AgentPanel::seed_explain_with_selection` directly.
- Refactor each verb's handler to:
  1. Build the preamble via `Preamble::build`.
  2. Construct the verb's prompt prefix
     (`"Explain this selection:\n"`, etc.) as the user-message body
     plus the selection text from `SelectionSource`.
  3. Call `harness.run_turn` with a fixed cancellation token (the
     verbs don't have an overlay yet, so cancellation is via the
     agent panel's own UI — or via the focused-pane Esc once the
     verb routes through the contextual overlay; spec follow-up).
- Behaviour preserved:
  - The verb still opens the agent panel after a successful turn.
  - The seeded prompt still lands in the agent's message editor;
    the agent panel surface itself is the place the user can chat
    multi-turn from.
- The previous direct call to
  `AgentPanel::seed_explain_with_selection` is removed; the
  vendored helper either stays (used internally by the harness's
  agent-panel adapter) or is replaced by a thinner shim.
- Add an integration test that asserts the migration:
  `AgentExplain` over a selection produces a trace entry, the
  preamble was built, and the agent panel ended up focused with
  the seeded prompt visible.

## Acceptance

- All three verbs route through `Harness::run_turn`.
- The phase-3 acceptance tests for the cross-pane verbs continue
  to pass.
- The harness trace contains an entry for every verb invocation.
- `cargo test -p codon-agent` passes.
