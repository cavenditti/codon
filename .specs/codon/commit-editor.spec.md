---
id: REQ:codon/commit-editor
type: requirement
status: accepted
version: 0.1.0
level: MAY
summary: >
  AI-generated commit messages are reachable from the keymap; the
  generation itself reuses Zed's existing language_model integration.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-3]
---

# Commit message generation

## Context

Zed already wires `git::GenerateCommitMessage` through
`language_model::LanguageModelRegistry` (see `git_panel.rs:2495`). The
codon-only piece is exposing this in the default keymap, plus
considering whether to route commit messages through agent threads
(deferred).

:::{requirement id="commit-editor" level="MAY"}
The system SHOULD:

- {#c-keymap} bind `cmd-k g m` to `git::GenerateCommitMessage` in the
  default codon keymap
- {#c-agent-route} optionally route AI commit message generation
  through agent threads instead of standalone LLM calls — deferred
  pending the agent-as-pane conversion
:::
