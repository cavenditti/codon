---
id: REQ:codon/git-pane
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Fork Zed's git crate and ship status / log / diff / hunk-staging
  as panes. Cross-pane git.blame.show verb.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-4]
---

# Git panes

:::{requirement id="git-pane" level="SHOULD"}
The system SHOULD provide:

- {#c-status} a status pane (working tree summary, staged / unstaged)
- {#c-log} a log pane with branch / graph rendering
- {#c-diff} a diff pane built on Zed's concrete `language::Buffer`
- {#c-hunk-staging} keyboard hunk staging from the diff pane
- {#c-blame-verb} a cross-pane `git.blame.show` verb that consumes
  `Selection::Hunks`
:::
