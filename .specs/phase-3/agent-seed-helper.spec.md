---
id: TASK:phase-3/agent-seed-helper
type: task
status: accepted
version: 0.1.0
summary: >
  Public AgentPanel::seed_explain_with_selection(prefix, ...) added to vendored agent_ui.
owners: [carlo]
progress: done
refines:
  - REQ:codon/agent-pane#c-seed-helper
---

# Agent seed helper

Promotes ConversationView::insert_selections and ThreadView::active_editor from pub(crate) to pub. Lives at [vendor/zed/crates/agent_ui/src/agent_panel.rs](spec:src:vendor/zed/crates/agent_ui/src/agent_panel.rs).
