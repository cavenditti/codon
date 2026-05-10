---
id: TASK:phase-3/commit-keymap
type: task
status: accepted
version: 0.1.0
summary: >
  cmd-k g m bound to git::GenerateCommitMessage.
owners: [carlo]
progress: done
refines:
  - REQ:codon/commit-editor#c-keymap
---

# AI commit message keymap

Default keymap entry only — Zed already implements the generation in git_panel.rs:2495 via language_model::LanguageModelRegistry.
