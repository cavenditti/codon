---
id: TASK:phase-4/unified-config-toml-schema
type: task
status: accepted
version: 0.0.1
summary: >
  Define the [settings.*] TOML schema mirroring Zed's SettingsContent
  struct. Document quirks (heterogeneous arrays → TOML inline-tables,
  comment preservation rules).
owners: [carlo]
progress: done
refines:
  - REQ:codon/unified-config#c-toml-schema
---

# TOML schema for `[settings.*]`

## What ships

A short design document (lives in the codon-config crate's
`README.md` or as a doc comment in `lib.rs`) capturing:

- One-to-one mapping rules from `SettingsContent` to TOML.
- Quirk: heterogeneous arrays (e.g. Zed keymap entries mixing
  contexts and bindings) are represented as TOML inline tables in
  an array-of-tables.
- Quirk: per-language overrides use `[settings.languages."c++"]`
  syntax — TOML quoted keys handle the `++`.
- Comment-roundtrip policy: comments on lines unaffected by a
  programmatic edit are preserved (line-oriented rewrite); comments
  on rewritten lines are dropped (settings_ui owns the canonical
  formatting of edited keys).
- Null / Option semantics: TOML lacks null. Optional Zed settings are
  represented by the key's absence, never by an explicit `= null`.

## Why a separate task

Doing the schema design before the config crate avoids late-stage
discovery that some Zed setting doesn't roundtrip cleanly. The
schema doc is the contract between the loader, the settings_ui
rewire, and the migration step.

## Files

- `crates/codon-config/README.md` (new) — schema reference.
- Reference: `vendor/zed/crates/settings_content/src/settings_content.rs`
  has the canonical `SettingsContent` definition.

Small (≤200 LOC of doc), but unblocks the rest of the workstream.
