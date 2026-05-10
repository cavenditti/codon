---
id: TOPIC:topics/phase-3
type: topic
status: accepted
version: 0.1.0
summary: >
  Agent pane type, cross-pane agent verbs (explain/summarize/refactor),
  inline assistant, and AI-generated commit messages.
owners: [carlo]
---

# Phase 3 — Agent, inline assistant, commit editor

Bring Zed's agent and inline_assistant into codon's modal model. The
agent should be a first-class pane (not a sidebar), reachable via
selection-first verbs: select code, hit `cmd-k a e` and the agent
opens with the selection seeded as context.

Refining requirements:

- [REQ:codon/agent-pane](spec:REQ:codon/agent-pane) — agent as a pane
  type and the cross-pane verbs.
- [REQ:codon/commit-editor](spec:REQ:codon/commit-editor) — AI commit
  message generation exposed in keymap.
