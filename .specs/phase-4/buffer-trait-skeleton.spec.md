---
id: TASK:phase-4/buffer-trait-skeleton
type: task
status: accepted
version: 0.2.0
summary: >
  Wontdo 2026-05-13 — the trait was originally created (the crate
  shipped in phase-4) but never earned a consumer. With Helix-as-
  engine integration removed from the roadmap, REQ:codon/buffer-
  trait is superseded and the crate was removed. Treating the task
  as wontdo so the spec graph reflects a clean state.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/buffer-trait#c-trait-definition
---

# codon-buffer crate skeleton (wontdo)

## Wontdo (2026-05-13)

The crate physically shipped (trait + `impl Buffer for
language::Buffer` forwarder, 95 LOC at `crates/codon-buffer/`) but
never reached the point of being useful — no codon site ever took
a `&dyn codon_buffer::Buffer`, no second implementer was added,
and the consumer-rewire turned out to need similar abstractions
across many adjacent types (Entity erasure, BufferSnapshot, edit
pathways), which made the cost / benefit lopsided. With Helix
integration off the roadmap the second consumer will never come,
so the work shipped → got removed. Treating as wontdo for
spec-graph coherence even though commits exist.

The original planning notes are kept below for historical
traceability.

## Original framing

Stand up a new `crates/codon-buffer` crate that defines a `Buffer`
trait — the minimum dependency surface every consumer needs.

## Trait surface (from grep of language::Buffer callsites)

Read-only (essential):

- `snapshot() -> BufferSnapshot`
- `text_snapshot() -> text::BufferSnapshot`
- `file() -> Option<&Arc<dyn File>>`
- `language() -> Option<&Arc<Language>>`
- `is_dirty() -> bool`
- `saved_version() -> &Global`
- `saved_mtime() -> Option<MTime>`
- `encoding() -> &Encoding`
- `has_bom() -> bool`
- `text() -> String` (on snapshot)
- `len() -> usize`

Anchor / edit (essential):

- `anchor_before(pos) -> Anchor`
- `anchor_after(pos) -> Anchor`
- `edit(edits, cursor_intent, cx)`
- `capability() -> Capability`

## Files to create / edit

- `crates/codon-buffer/Cargo.toml` (workspace deps: `language`, `text`, `gpui`)
- `crates/codon-buffer/src/codon_buffer.rs` — trait + re-exports
- `Cargo.toml` workspace members + path dep entry

The trait stays in its own crate so consumers can pull it in without
the heavy `language` crate's transitive deps when only the trait is
needed. The actual impls (Zed, Helix) live in dedicated tasks.
