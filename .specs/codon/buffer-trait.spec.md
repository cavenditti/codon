---
id: REQ:codon/buffer-trait
type: requirement
status: superseded
version: 0.2.0
level: SHOULD
summary: >
  Superseded 2026-05-13 — Helix-as-engine integration is wontdo.
  Codon uses Zed's built-in Helix-style modal editing (vim with
  `helix_default` force-enabled), not a vendored helix-editor
  engine plugged in as the buffer backend. With no second consumer
  ever planned, the codon_buffer::Buffer abstraction has no payoff
  over using `language::Buffer` directly. Removal of the
  codon-buffer crate is tracked under a follow-up cleanup.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-4]
---

# Buffer trait (superseded)

## Context (historical)

Originally drafted to support plugging Helix's `helix_view::Document`
into codon as an alternate text-storage backend, decoupling
consumers (editor / search / agent / git) from `language::Buffer`.
The integration was deferred at the end of Phase 4 and is now
wontdo — codon adopted Zed's vim+Helix-default mode wholesale
instead of vendoring helix-editor and rewiring buffer plumbing.

:::{requirement id="buffer-trait" level="SHOULD"}
~~The system SHOULD provide:~~

- ~~{#c-trait-definition} a `codon_buffer::Buffer` trait capturing the
  minimal surface used by editor / search / agent / git~~
- ~~{#c-zed-impl} an impl for `language::Buffer` (default)~~
- ~~{#c-helix-impl} an impl for `helix_view::Document` or a wrapper~~
- ~~{#c-consumer-rewire} editor/search/agent/git crates take
  `&dyn Buffer` instead of `&language::Buffer` at trait boundaries~~

**Status:** all clauses superseded. The trait + zed-impl shipped
under phase-4 (see `crates/codon-buffer/`) but has zero consumers
and no path to one. The crate is slated for removal.
:::
