---
id: TASK:phase-3/agent-pane-conversion
type: task
status: accepted
version: 0.1.0
summary: >
  Convert AgentPanel from impl Panel (sidebar) to impl Item (workspace pane).
owners: [carlo]
progress: done
refines:
  - REQ:codon/agent-pane#c-pane-conversion
---

# AgentPanel pane conversion (deferred)

The file is 2,400+ lines. Conversion touches dock state, serialization, focus handling, and every caller of panel::<AgentPanel>(). The cross-pane verbs (which are the user-facing payoff) work today against the panel-shaped agent. Defer until a structural change is unavoidable.
