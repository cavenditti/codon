---
id: REQ:codon/agent-pane
type: requirement
status: draft
version: 0.1.0
level: SHOULD
summary: >
  The agent is a first-class pane with selection-aware verbs that send
  the active selection to a seeded message editor.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-3]
---

# Agent pane and cross-pane verbs

## Context

Phase 1 set up `Selection` + `SelectionSource` + `ActionAcceptsRegistry`
to filter the command palette by what's currently selected. Phase 3
exercises that machinery: the agent has verbs that consume selections
from any pane.

The agent is currently `impl workspace::Panel` (sidebar). Converting it
to `impl workspace::Item` (pane) is on the table but invasive — the
file is 2,400+ lines and conversion touches dock state, serialization,
focus handling, and every caller of `panel::<AgentPanel>()`.

:::{requirement id="agent-pane" level="SHOULD"}
The system SHOULD provide:

- {#c-cross-pane-verbs} actions `AgentExplain`, `AgentSummarize`,
  `AgentRefactor` that capture the current workspace selection, focus
  the agent, and seed its message editor with a prompt prefix
- {#c-action-accepts} each verb registered against
  `ActionAcceptsRegistry` for `Text`, `File`, `Dir`, `Hunk`,
  `Diagnostic`, `Block`
- {#c-seed-helper} a small public helper on `AgentPanel` that other
  crates can call to seed a prompt (without owning the agent's
  pub(crate) internals)
- {#c-pane-conversion} the agent eventually MUST be a workspace pane
  rather than a sidebar panel — this is a follow-up Phase 3 task,
  blocked on a structural change that can't be expressed as additive
  helpers
:::
