---
id: TASK:phase-4/unified-config-config-crate
type: task
status: accepted
version: 0.0.1
summary: >
  New crates/codon-config crate that parses codon.toml, translates
  [settings.*] to serde_json::Value matching SettingsContent, and
  feeds it into SettingsStore via the existing user-settings path.
owners: [carlo]
progress: done
refines:
  - REQ:codon/unified-config#c-config-crate
---

# `codon-config` crate

## What ships

A new `crates/codon-config/` workspace member with:

- `Cargo.toml` (workspace deps: `toml`, `serde`, `serde_json`,
  `settings`, `paths`, `gpui`, `anyhow`, `log`).
- `src/codon_config.rs` — public entry: `load_user_config(cx)`,
  `register(cx)`.
- `src/toml_to_json.rs` — translation from `toml::Value` to
  `serde_json::Value`.

Behaviour:

1. Read `~/.config/codon/codon.toml`.
2. Extract the `[settings]` sub-table and translate to JSON via
   `toml_to_json::translate`.
3. Hand the JSON string to Zed's `SettingsStore::set_user_settings`
   (or whichever public entry exists — task includes adding a small
   public hook if one isn't already exposed).
4. Errors logged via `log::warn!`; never panic on malformed input.

## Reference points

- [`vendor/zed/crates/settings/src/settings_store.rs`](spec:src:vendor/zed/crates/settings/src/settings_store.rs)
  — `SettingsStore::load_settings` is the current JSON entry. Find
  the smallest public surface that accepts pre-parsed JSON.
- [`vendor/zed/crates/settings_content/src/settings_content.rs`](spec:src:vendor/zed/crates/settings_content/src/settings_content.rs)
  — the target shape for the translated JSON.
- [`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
  — pattern for a codon-owned TOML loader.

## Tests

- TOML roundtrip: parse a fixture, translate to JSON, assert against
  a serde-deserialized `SettingsContent`.
- Comment-only TOML: assert it parses and yields an empty settings
  object.

Effort: medium. ~300 LOC including tests.
