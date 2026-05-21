---
id: TASK:phase-22/command-history-store
type: task
status: accepted
version: 0.1.0
summary: >
  New `codon-command-history` crate housing the sqlite schema, FTS5
  virtual table over raw command_text + summaries + tags, and the
  Rust API (`HistoryStore::insert`, `search`, `recent_for_cwd`,
  `gc`). On-disk layout matches `c-store-sqlite`; raw bytes are
  stored as-is (same trust model as shell history).
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-history#c-store-sqlite
aspects: [schema, fts5-index]
---

# codon-command-history crate + sqlite store

## Plan

- New workspace member `crates/codon-command-history/` with
  lib-root `src/codon_command_history.rs`.
- Schema (sqlite, applied via migrations under
  `crates/codon-command-history/migrations/`):
  ```sql
  CREATE TABLE entries (
      id INTEGER PRIMARY KEY,
      ts_utc TEXT NOT NULL,           -- ISO 8601
      cwd TEXT NOT NULL,
      shell TEXT NOT NULL,
      command_text TEXT NOT NULL,     -- raw, same as shell history
      output_excerpt TEXT,            -- raw, may be NULL if not captured
      exit_code INTEGER,
      duration_ms INTEGER,
      summary_what TEXT,              -- NULL until summarized; NULL if llm_skipped
      summary_did TEXT,
      tags_json TEXT NOT NULL DEFAULT '[]',
      llm_skipped INTEGER NOT NULL DEFAULT 0,
      skip_reason TEXT                -- e.g. 'risky_redaction', 'budget_exhausted'
  );
  CREATE VIRTUAL TABLE entries_fts USING fts5(
      command_text, summary_what, summary_did, tags_json,
      content='entries', content_rowid='id'
  );
  -- Triggers keep FTS in sync.
  ```
- Dependency: `rusqlite` with the `bundled` + `fts5` features.
  Pin to a current minor — confirm during PR that it's already in
  a workspace `Cargo.toml` or add it carefully (Zed already
  vendors sqlite in places).
- Storage path: `~/.config/codon/command_history/<fingerprint>.
  sqlite`. The fingerprint helper is reused from
  `codon-memory`'s implementation (sibling task
  `memory-store-layout`) — extract to a shared crate
  (`codon-workspace-fingerprint`) if needed.
- API:
  ```rust
  pub struct HistoryStore { conn: rusqlite::Connection }
  impl HistoryStore {
      pub fn open(fp: &Fingerprint) -> Result<Self>;
      pub fn insert(&mut self, e: NewEntry) -> Result<EntryId>;
      pub fn update_summary(&mut self, id: EntryId, what: &str, did: &str, tags: &[String]) -> Result<()>;
      pub fn mark_skipped(&mut self, id: EntryId, reason: &str) -> Result<()>;
      pub fn search(&self, q: &SearchQuery) -> Result<Vec<EntryRow>>;
      pub fn recent_for_cwd(&self, cwd: &Path, n: usize) -> Result<Vec<EntryRow>>;
      pub fn gc(&mut self, max_entries: usize) -> Result<usize>;
  }
  ```
- The store API has no notion of redaction; rows are typed
  `String`, not `RedactedText`. Egress points (the summarizer,
  the `search_command_history` tool, the project-kb aggregator)
  are responsible for routing rows through
  `codon_redact::default_pipeline()` before constructing the
  LLM-call argument. The store is the data layer; redaction
  lives at the network/egress edge.

## Acceptance

- `cargo test -p codon-command-history` passes including a
  property test: insert 1k entries with mixed cwds, search by
  substring, assert ordering and recall.
- FTS5 search returns BM25-ranked results.
- A round-trip integration test: open, insert, reopen, read —
  the file persists across process restarts and the raw command
  text round-trips byte-for-byte.
