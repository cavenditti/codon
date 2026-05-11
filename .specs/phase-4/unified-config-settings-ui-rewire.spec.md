---
id: TASK:phase-4/unified-config-settings-ui-rewire
type: task
status: accepted
version: 0.0.1
summary: >
  Rewire Zed's in-app settings editor so writes land in codon.toml's
  [settings.*] table, with tree-sitter TOML preserving comments and
  formatting on unchanged keys.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/unified-config#c-settings-ui-rewire
---

# Rewire `settings_ui` to write TOML

## What ships

The in-app settings editor today calls `SettingsStore::update_settings_file`,
which uses `update_value_in_json_text` (tree-sitter JSON parser) to
mutate `~/.config/zed/settings.json` in place — comments and
formatting preserved. We need the same shape for TOML.

The codon-side implementation:

1. Add a write-back callback to `SettingsStore` (small vendored
   patch) that codon can override. When set, all writes route through
   the callback instead of `update_value_in_json_text`.
2. codon's callback implementation lives in `codon-config`:
   - Parse `codon.toml` via tree-sitter TOML grammar.
   - Apply the value change at the right `[settings.*.key]` path,
     line-oriented, preserving surrounding comments / whitespace.
   - Model after
     [`vendor/forge-spec/spec-cli/src/commands/task.rs::rewrite_progress`](spec:src:vendor/forge-spec/spec-cli/src/commands/task.rs)
     — same approach (line-by-line scan, mutate the target line,
     leave the rest byte-identical).

## Why this is the heaviest task

settings_ui's edit paths are scattered; auditing every callsite to
ensure it goes through the new callback rather than the JSON helper
is the bulk of the work. ~500 LOC of mostly mechanical changes.

## Phasing

If this slips, ship in two stages:

- **Read-only:** settings_ui renders TOML-derived state but writes
  are disabled or fall back to silently writing JSON to a sidecar
  file (`~/.config/codon/codon.json` as override). User can still
  edit `codon.toml` by hand.
- **Read-write:** the callback wires through and the sidecar is
  retired.

Effort: high.
