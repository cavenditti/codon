---
id: REQ:codon/buffer-trait
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Define a codon_buffer::Buffer trait that decouples consumers from
  Zed's concrete Buffer type, enabling plug-in of helix_view::Document.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-4]
---

# Buffer trait

## Context

Helix has a much more compact text-storage and edit-history model
than Zed. To bring Helix's `Document` into codon as the underlying
buffer, every consumer of `language::Buffer` needs to take an
abstraction instead.

:::{requirement id="buffer-trait" level="SHOULD"}
The system SHOULD provide:

- {#c-trait-definition} a `codon_buffer::Buffer` trait capturing the
  minimal surface used by editor / search / agent / git
- {#c-zed-impl} an impl for `language::Buffer` (default)
- {#c-helix-impl} an impl for `helix_view::Document` or a wrapper
- {#c-consumer-rewire} editor/search/agent/git crates take
  `&dyn Buffer` instead of `&language::Buffer` at trait boundaries
:::
