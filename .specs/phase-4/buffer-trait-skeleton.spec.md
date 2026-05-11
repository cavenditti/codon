---
id: TASK:phase-4/buffer-trait-skeleton
type: task
status: accepted
version: 0.0.1
summary: >
  New crates/codon-buffer with a Buffer trait capturing the minimal
  read + edit surface used by editor, search, agent_ui, and git_ui.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/buffer-trait#c-trait-definition
---

# codon-buffer crate skeleton

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
