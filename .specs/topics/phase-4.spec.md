---
id: TOPIC:topics/phase-4
type: topic
status: draft
version: 0.1.0
summary: >
  Define a Buffer trait that decouples codon panes from Zed's concrete
  Buffer type and rebuild the git pane on top of it.
owners: [carlo]
---

# Phase 4 — Buffer trait & git integration

The eventual goal is to plug Helix's `helix_view::Document` into codon
without forking every consumer. To do that, codon needs its own Buffer
trait that captures the dependencies Zed's editor / search / agent
crates take.

Refining requirements (deferred drafts):

- [REQ:codon/buffer-trait](spec:REQ:codon/buffer-trait)
- [REQ:codon/git-pane](spec:REQ:codon/git-pane)
- [REQ:codon/unified-config](spec:REQ:codon/unified-config)
