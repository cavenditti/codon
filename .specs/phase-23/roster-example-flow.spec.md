---
id: TASK:phase-23/roster-example-flow
type: task
status: accepted
version: 0.1.0
summary: >
  Ship a documented tiered example flow (orchestrator / implementer /
  reviewer / safety) with prompt files under assets/config/flows/,
  wired into codon.example.toml and compile-tested.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-roster#c-example-flow
assignee:
eta:
blocked_by: []
---

# Roster example flow

## Plan

- `assets/config/flows/tiered.rhai` + `assets/config/flows/prompts/`
  (`orchestrator.md`, `implementer.md`, `reviewer.md`, `safety.md`)
  adapting Carlo's opencode roster (orchestrator self-plans and
  delegates; cheap implementer with misclassification escape; stronger
  read-only reviewer — generator/verifier asymmetry; safety classifier
  with escalation) to codon's flow API. Morph-editing and
  browser-specialist agents are deliberately dropped.
- Handoffs are report-enabled (`report: true`); shell goes to the
  implementer gated by `safety_for("shell", safety, reviewer)`.
- `codon.example.toml` gains a commented `[agent_harness]` section
  showing `active_flow = "tiered"` + `flow_paths` + example
  `shell_permissions` rules.
- A test compiles the shipped flow file verbatim (with prompt files)
  so the example cannot drift from the flow API.

## Acceptance

The example flow + prompts exist under assets/config/flows/, are
referenced from codon.example.toml, and a `cargo test -p codon-agent`
test loads and compiles them successfully.
