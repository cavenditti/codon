---
id: TASK:phase-3/agent-action-accepts
type: task
status: accepted
version: 0.1.0
summary: >
  Each agent verb registered against ActionAcceptsRegistry for Text/File/Dir/Hunk/Diagnostic/Block.
owners: [carlo]
progress: done
refines:
  - REQ:codon/agent-pane#c-action-accepts
---

# Wire ActionAcceptsRegistry

Performed in codon_agent::actions::register, called from apps/codon/src/main.rs after codon_mode::init.
