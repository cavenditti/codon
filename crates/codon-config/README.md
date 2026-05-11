# codon-config

Unified configuration crate for codon. Loads `~/.config/codon/codon.toml`
and feeds the `[settings.*]` sub-tree to Zed's `SettingsStore` while
exposing the `[bindings.*]` sub-tree to `codon-keymap`. See
[`REQ:codon/unified-config`](../../.specs/codon/unified-config.spec.md)
for the requirement.

## File layout on disk

```
~/.config/codon/
  codon.toml       — single source of truth (this crate's input)
```

Legacy files (`~/.config/zed/settings.json`,
`~/.config/codon/keymap.toml`) are read once during migration on first
launch (see
[`TASK:phase-4/unified-config-migration`](../../.specs/phase-4/unified-config-migration.spec.md))
and afterwards ignored.

## Schema reference

`codon.toml` has two top-level tables. Both are optional — an empty
file is valid and yields all-default codon behaviour.

```toml
[settings]                          # mirrors Zed's SettingsContent
buffer_font_family = "JetBrains Mono"
buffer_font_size = 14

[settings.theme]
mode = "system"
light = "One Light"
dark = "One Dark"

[settings.languages.rust]           # per-language override
formatter = "language_server"

[bindings.global]                   # codon-keymap format, unchanged
"cmd-w" = "codon_session::SafeCloseActiveItem"
"cmd-shift-p" = "command_palette::Toggle"

[bindings.file_manager.normal]
"/" = "file_manager::FuzzyFilter"
```

### `[settings]` — one-to-one with `SettingsContent`

The canonical Zed struct is at
`vendor/zed/crates/settings_content/src/settings_content.rs::SettingsContent`.
It's `#[serde(flatten)]`ed across several sub-structs (`ProjectSettingsContent`,
`EditorSettingsContent`, `ThemeSettingsContent`, `TerminalSettingsContent`,
…). The codon-config translator walks the parsed `toml::Value` and
produces a `serde_json::Value` that round-trips through
`serde_json::from_value::<SettingsContent>`.

Mapping rules:

| Zed JSON shape                  | codon TOML shape                                   |
|---------------------------------|----------------------------------------------------|
| top-level object                | `[settings]` table                                 |
| nested object `{"a": {"b": 1}}` | `[settings.a]` table with `b = 1`                  |
| string / number / bool          | TOML string / integer-or-float / bool              |
| array of objects                | TOML array of inline tables                        |
| per-language override           | `[settings.languages.<name>]`; quoted for `c++`    |
| optional missing value          | key absent (TOML has no `null`)                    |

### `[bindings]` — codon-keymap format unchanged

The `[bindings.*]` sub-tree is exactly the shape `codon-keymap`'s
existing loader at
`crates/codon-keymap/src/keymap.rs` parses from
`~/.config/codon/keymap.toml`. Three pane-scoped sub-tables and a
global one:

- `[bindings.global]`
- `[bindings.editor.{normal,insert}]`
- `[bindings.terminal.{normal,insert}]`
- `[bindings.file_manager.{normal,insert}]`

Action names use Zed's namespaced format (`codon_session::SessionNew`,
`workspace::ActivatePaneLeft`, `vim::ResizePaneLeft`, …). The keymap
loader resolves them to typed `KeyBinding` values.

## Quirks worth knowing

- **No `null` in TOML.** Zed settings that accept `null` as a meaningful
  default (e.g. `"buffer_font_fallbacks": null`) are represented in
  TOML by omitting the key entirely. The translator never emits
  `serde_json::Value::Null` for an omitted key — downstream defaults
  apply.
- **Heterogeneous arrays.** Zed JSON occasionally has arrays mixing
  contexts and bindings (`keymap` style). For the `[settings]` tree
  this is rare; when it appears, use a TOML array-of-tables where each
  table carries the heterogeneous fields:

  ```toml
  [[settings.experimental_keymap]]
  context = "Editor"
  bindings = { "ctrl-x" = "Cut" }
  ```

- **Comment policy.** The first migration writes a `codon.toml`
  generated from the user's existing JSON/TOML — JSONC comments on
  edited keys do not survive (TOML and JSON sort keys differently).
  Programmatic edits performed by the in-app settings editor
  (`unified-config-settings-ui-rewire`) preserve comments on lines
  *not* touched by the edit, via a line-oriented rewrite (same
  approach as `vendor/forge-spec/spec-cli/src/commands/task.rs::rewrite_progress`).
- **Per-language overrides.** `[settings.languages.rust]` works as-is;
  language names with special characters use TOML quoted keys:
  `[settings.languages."c++"]`.
- **Theme.** `theme` can be a string (`theme = "One Dark"`) or a table
  (`[settings.theme] mode = "system" light = "..." dark = "..."`).
  Both forms are valid JSON in Zed; the translator preserves either.

## What this crate ships in subsequent tasks

| Task                                                   | Adds                                                |
|--------------------------------------------------------|-----------------------------------------------------|
| `unified-config-toml-schema` (this commit)             | crate skeleton + this README                        |
| `unified-config-config-crate`                          | `toml_to_json::translate` + `load_user_config`      |
| `unified-config-merge-keymap`                          | `codon-keymap` reads `[bindings.*]` from `codon.toml` |
| `unified-config-watch-reload`                          | file watcher + debounced reload                     |
| `unified-config-migration`                             | one-shot import from legacy files                   |
| `unified-config-example-config`                        | `assets/config/codon.example.toml`                  |
| `unified-config-settings-ui-rewire`                    | in-app editor writes TOML in place                  |
