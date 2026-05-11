---
id: TASK:phase-4/buffer-trait-helix-impl
type: task
status: accepted
version: 0.0.1
summary: >
  Implement codon_buffer::Buffer for helix_view::Document. Wontdo
  pending a separate Helix-vendoring phase — the trait surface is
  designed to be Helix-compatible so the impl is purely additive
  when Helix lands.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/buffer-trait#c-helix-impl
---

# Buffer impl for helix_view::Document (wontdo for now)

## Status

Helix is not vendored under `vendor/` today. Implementing a
`codon_buffer::Buffer` for `helix_view::Document` requires either:

1. Adding `helix-core` and `helix-view` as crates.io deps, or
2. Vendoring the relevant Helix subtree.

Both are phase-sized efforts on their own (Helix's text model is rope-
based with a different transaction system; mapping it onto Zed's
`Anchor` + `Edit` semantics is non-trivial).

## What stays compatible

The trait skeleton (see
[TASK:phase-4/buffer-trait-skeleton](spec:TASK:phase-4/buffer-trait-skeleton))
is deliberately shaped to accommodate `helix_view::Document`:

- Read methods are all snapshot-returning, which Helix's `Rope::clone`
  satisfies cheaply.
- The edit method takes a slice of edits, which maps onto Helix's
  `Transaction` building.
- File / language are `Option<…>` so Helix's lighter document type can
  return `None` for now.

## Revisit when

A future phase vendors Helix end-to-end. At that point, this task
moves from `wontdo` to `pending` and gets a real implementation. The
existing Zed impl + consumer-rewire work in Phase 4 is unaffected.
