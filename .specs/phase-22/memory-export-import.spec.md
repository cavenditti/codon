---
id: TASK:phase-22/memory-export-import
type: task
status: accepted
version: 0.1.0
summary: >
  Add `codon_memory::Export` and `codon_memory::Import` actions that
  tarball the current workspace's memory store under
  `<workspace>/.codon/memories.tar` and round-trip it back. Optional
  for phase-22 ship — defer if scope tightens.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shared-memory#c-export-import
---

# Memory export / import

## Plan

- New module `crates/codon-memory/src/export.rs`.
- `codon_memory::Export` action:
  - Pack every file in the current workspace's store directory
    into a tar at `<workspace_root>/.codon/memories.tar`.
  - Use the `tar` crate (already in vendored Zed deps via the
    extension system).
  - Add `.codon/` to `.gitignore` automatically if it isn't there
    yet (with a one-line toast notifying the user).
- `codon_memory::Import` action:
  - Read `<workspace_root>/.codon/memories.tar`, unpack into a
    temp dir, validate each entry's frontmatter, then move into
    the store.
  - On collision (same id), prefer the newer `created` timestamp
    and surface a one-line toast: "kept newer / dropped older for
    <title>".
- Lifecycle marker: this task is OPTIONAL for the phase-22 ship.
  If scope tightens, mark it `deferred` via `spec defer` without
  blocking the REQ from reaching done — the REQ's clause is
  `c-export-import` and the REQ itself is `SHOULD`, so deferring
  one optional clause is consistent.

## Acceptance

- Export of a store with 3 memories produces a tarball;
  unpacking it yields 3 valid memory files.
- Import on a fresh store reproduces the 3 memories.
- Collision rule: a newer-timestamp entry wins; toast surfaces.
- `cargo test -p codon-memory` passes.
