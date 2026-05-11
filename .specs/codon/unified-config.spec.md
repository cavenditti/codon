---
id: REQ:codon/unified-config
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Single user config file at ~/.config/codon/codon.toml that carries
  both [settings.*] (Zed-equivalent settings) and [bindings.*] (codon
  keymap). The in-app settings editor edits this file directly.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-4]
---

# Unified TOML config

## Context

Codon today has two user config files:

- `~/.config/zed/settings.json` — Zed's settings tree (JSONC, edited
  via Zed's in-app settings UI).
- `~/.config/codon/keymap.toml` — codon's bindings, parsed by
  `crates/codon-keymap/src/keymap.rs`.

Two files, two formats. New users have to learn both. The in-app
settings editor doesn't know codon's bindings exist; codon's TOML
loader doesn't know about settings.

Merge into a single `~/.config/codon/codon.toml`:

```toml
[settings]
buffer_font_family = "JetBrainsMono Nerd Font"
buffer_font_size = 14

[settings.theme]
mode = "system"
light = "One Light"
dark = "One Dark"

[settings.languages.rust]
formatter = "language_server"

[bindings.global]
"cmd-w" = "codon_session::SafeCloseActiveItem"

[bindings.file_manager.normal]
"/" = "file_manager::FuzzyFilter"
```

The `[settings.*]` tree mirrors Zed's `SettingsContent` struct (serde
+ schemars). The `[bindings.*]` tree is codon-keymap's existing format,
unchanged.

:::{requirement id="unified-config" level="MUST"}
The system MUST provide:

- {#c-toml-schema} a TOML schema mirroring Zed's `SettingsContent`
  one-to-one, with quirks documented (heterogeneous arrays via TOML
  inline-tables, comments preserved on roundtrip)
- {#c-config-crate} a `crates/codon-config` crate that parses
  `codon.toml`, translates `[settings.*]` to `serde_json::Value`
  matching `SettingsContent`, and hands the result to Zed's
  `SettingsStore` via the user-settings entry point
- {#c-merge-keymap} `codon-keymap` reads `[bindings.*]` out of the
  same `codon.toml` (loader gains a second path; the file format is
  the existing TOML keymap shape, just embedded under a parent key)
- {#c-settings-ui-rewire} the in-app settings editor (`settings_ui`)
  writes back through the TOML translation layer, preserving comments
  and formatting on unchanged keys (line-oriented rewrite à la
  `spec-cli::task::rewrite_progress`)
- {#c-watch-reload} hot-reload on `codon.toml` change reuses
  `SettingsStore::watch_settings_files` for settings + the existing
  codon-keymap reload path for bindings
- {#c-migration} first-launch migration: if `codon.toml` is absent
  but `~/.config/zed/settings.json` or `~/.config/codon/keymap.toml`
  exists, auto-convert into a merged `codon.toml`; leave the source
  files in place with a deprecation header so the user can roll back
- {#c-example-config} `assets/config/codon.example.toml` replaces
  `keymap.example.toml`, showcasing settings + bindings together with
  inline documentation
:::

## Approach

The bridge crate translates between two value models:

1. TOML → `toml::Value` (codon's input)
2. `toml::Value` → `serde_json::Value` shaped like Zed's
   `SettingsContent` (handed to `SettingsStore`)

This avoids forking Zed's `SettingsStore::load_settings` or its JSONC
parser — both are merge-conflict hot spots when pulling upstream Zed.
The settings_ui rewire is the heaviest piece: tree-sitter TOML
grammar + the line-oriented rewrite pattern already in use by
spec-cli (see
[`vendor/forge-spec/spec-cli/src/commands/task.rs::rewrite_progress`](spec:src:vendor/forge-spec/spec-cli/src/commands/task.rs))
gets us comment-preserving writes.
