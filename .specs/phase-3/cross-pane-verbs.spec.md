---
id: TASK:phase-3/cross-pane-verbs
type: task
status: accepted
version: 0.1.0
summary: >
  AgentExplain/Summarize/Refactor capture the active selection, focus the agent panel, and seed the message editor with a prompt prefix.
owners: [carlo]
progress: done
refines:
  - REQ:codon/agent-pane#c-cross-pane-verbs
---

# Cross-pane agent verbs

Implemented in [crates/codon-agent/src/actions.rs](spec:src:crates/codon-agent/src/actions.rs). Uses the new AgentPanel::seed_explain_with_selection helper from the vendored agent_ui crate.
