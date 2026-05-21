---
id: TASK:phase-22/memory-store-layout
type: task
status: accepted
version: 0.1.0
summary: >
  Create the `codon-memory` crate, define the on-disk layout
  (`~/.config/codon/memories/<fingerprint>/*.md`), workspace
  fingerprinting, on-open index, and per-memory + per-store size
  guards.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shared-memory#c-store-layout
  - REQ:codon/agent-shared-memory#c-fingerprint
  - REQ:codon/agent-shared-memory#c-index-on-open
  - REQ:codon/agent-shared-memory#c-shape-budget
aspects: [on-disk-layout, fingerprint, index-on-open, size-guards]
---

# codon-memory crate + store layout

## Plan

- New workspace member `crates/codon-memory/` (Cargo.toml,
  `src/codon_memory.rs` as lib-root per the codon naming rule).
- Types:
  - `pub struct MemoryEntry { id: MemoryId, title: String, body:
    String, tags: Vec<String>, created: chrono::DateTime<Utc>,
    pinned: bool }`.
  - `pub struct MemoryStore { root: PathBuf, fingerprint: String,
    entries: HashMap<MemoryId, MemoryEntry> }`.
- On-disk layout:
  - Root: `dirs::config_dir()/codon/memories/<fingerprint>/`.
  - One file per memory: `<id>.md` where `<id>` is a slug derived
    from the title plus a 4-char random suffix to avoid collisions.
  - File format: YAML frontmatter (title, created, tags, pinned)
    + markdown body.
- Fingerprint:
  - Canonicalise the workspace root path via
    `std::fs::canonicalize`.
  - SHA-256 over the canonical UTF-8 bytes, hex-encode the first
    16 chars. Deterministic across renames of unrelated paths;
    rename of the workspace root itself is intentionally a fresh
    store (documented in the REQ).
- Index on open:
  - `MemoryStore::load(fingerprint, root) -> Result<Self>` walks
    the directory, parses each file, returns the in-memory
    `HashMap`. No separate cache file.
- Size guards:
  - Per-memory body ≤ 4 KiB enforced on `write`; returns
    `MemoryError::TooLarge`.
  - Total store size soft-warning at 256 KiB — exposed via
    `MemoryStore::warn_if_oversize() -> Option<usize>`. The picker
    (sibling task) renders the warning.

## Acceptance

- A new workspace with no memories returns an empty store on open
  (no error, no directory creation until the first write).
- Round-trip: write a memory, drop the store, reopen, the memory
  is present with identical contents.
- A body larger than 4 KiB returns `MemoryError::TooLarge` and the
  file is not written.
- `cargo test -p codon-memory` passes.
