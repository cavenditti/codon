---
id: TASK:phase-5/config-writeback-toml-edit
type: task
status: accepted
version: 0.1.0
summary: >
  Replace the line-by-line string splicer in codon-config's writeback
  with `toml_edit`'s AST so user comments, whitespace, and ordering
  survive every edit — and so the splice no longer breaks on
  commented-out `[settings]` headers or unusual TOML layouts. Read +
  write paths route through `Arc<dyn fs::Fs>` (`fs.metadata` +
  `fs.load` + `fs.atomic_write` + `fs.create_dir`) instead of
  `std::fs::*`. Decor preservation handles the case where a
  free-standing comment block sits above the `[settings]` header.
  Symmetry on `toml_to_json` not attempted — the JSON-emit path is
  read-only and reads via `toml::Value` are fine as-is.
owners: [carlo]
progress: done
refines:
  - REQ:codon/unified-config
  - REQ:codon/code-quality#c-fs-trait-purity
aspects: [toml-edit-writeback, fs-trait-routing]
---

# Migrate writeback to `toml_edit`

## What ships

[`crates/codon-config/src/writeback.rs`](spec:src:crates/codon-config/src/writeback.rs)
currently rewrites the user's `~/.config/codon/codon.toml` by scanning
the file line-by-line (around lines 136–163), looking for the
`[settings]` table header, tracking a manual cursor, and splicing
the new block in by string concatenation. This works for the happy
path — a single `[settings]` table, no surprises — but it has three
known cliffs:

1. A commented `# [settings]` line earlier in the file gets matched
   as a header.
2. A file with no trailing newline produces a splice that fuses two
   lines together.
3. Sub-table headers (`[settings.theme]`) interact awkwardly with
   the "where does the `[settings]` block end" heuristic when the
   user has hand-edited the file.

`toml_edit` (already in the workspace's transitive dep tree via
`vendor/zed/crates/settings`) parses TOML to a format-preserving AST,
preserves comments and whitespace, and exposes `Document::insert` /
`Document::remove` for table-level edits. That's the right tool.

## What changes

- Parse the existing file with `toml_edit::DocumentMut`.
- Mutate the `settings` table in-place (replace entire sub-tree or
  per-key patch — match whatever the in-app settings UI already does
  in vendored Zed for consistency).
- Render back with `Document::to_string()` and write atomically.
- The `[bindings.*]` sub-tree is preserved by definition — we only
  touch `[settings.*]`.

While you're in there:

- The async writeback function takes `Arc<dyn fs::Fs>` but calls
  `std::fs::read_to_string` / `std::fs::write` directly. Route the
  read + write through the trait. This is the
  REQ:codon/code-quality#c-fs-trait-purity rule for this site.
- The translation logic in
  [`crates/codon-config/src/migrate.rs`](spec:src:crates/codon-config/src/migrate.rs)
  conflates the JSON→TOML translation with file I/O. After this
  task, the translation should be a pure function `(serde_json::Value)
  → toml_edit::Item` callable from a test. Tests live next to it.

## File anchors

- [`crates/codon-config/src/writeback.rs`](spec:src:crates/codon-config/src/writeback.rs)
- [`crates/codon-config/src/migrate.rs`](spec:src:crates/codon-config/src/migrate.rs)
- [`crates/codon-config/src/toml_to_json.rs`](spec:src:crates/codon-config/src/toml_to_json.rs)
  — the inverse direction; should also move to `toml_edit` for
  symmetry, but only if cost is small.

## Acceptance

- Writing the same setting twice produces the same file content (no
  whitespace churn, comments preserved).
- A file with a commented-out `# [settings]` line earlier than the
  real one is handled correctly.
- A round-trip property test: arbitrary `SettingsContent` →
  writeback → re-parse → equal `SettingsContent`. At least 10 hand-
  written fixture cases including: file with leading comment block,
  file with `[bindings]` before `[settings]`, file with
  `[settings.theme]` sub-table only.
- No call to `std::fs::*` remains in the async paths of
  codon-config; all routed via `Arc<dyn fs::Fs>`.

## Out of scope

- Schema migrations between codon-config versions — separate concern.
- A full settings-editor UI — that's a phase-5 stretch, not this.

Effort: medium. The `toml_edit` rewrite is ~120 LOC swap-in; the
fs-trait routing and the round-trip tests are the larger part of the
diff (~200 LOC of tests).
