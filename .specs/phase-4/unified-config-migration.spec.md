---
id: TASK:phase-4/unified-config-migration
type: task
status: accepted
version: 0.0.1
summary: >
  On first launch with no codon.toml, auto-convert from existing
  Zed settings.json and/or codon keymap.toml into a merged codon.toml.
  Leave the source files in place with a deprecation header.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/unified-config#c-migration
---

# Config migration

## What ships

`codon-config` gains a `migrate_if_needed()` function called once
during `codon_config::init`:

1. If `~/.config/codon/codon.toml` exists, return early — nothing to do.
2. Read `~/.config/zed/settings.json` (JSONC) and/or
   `~/.config/codon/keymap.toml` (TOML) if present.
3. Translate the JSON to TOML for the `[settings]` tree; embed the
   existing keymap TOML under `[bindings]` unchanged.
4. Write the result to `~/.config/codon/codon.toml`.
5. Prepend a `# DEPRECATED: superseded by ~/.config/codon/codon.toml`
   header to each source file (commented appropriately for JSON vs
   TOML) without otherwise changing them, so the user can revert by
   deleting `codon.toml`.

## Edge cases

- JSONC comments: not all comments survive translation (the order
  may shift, since TOML and JSON sort keys differently). Acceptable
  one-time loss; log a hint pointing the user at the deprecation
  header.
- Conflict between Zed settings and Zed defaults: only the keys the
  user actually overrode are migrated (those present in
  `settings.json`; defaults stay implicit).
- Missing source files: migration writes a `codon.toml` containing
  only the embedded codon defaults (so subsequent launches behave
  identically).

## Files

- New: `crates/codon-config/src/migrate.rs`.
- Touchpoint: codon-config's `init` calls `migrate_if_needed` before
  `load_user_config`.

Medium. ~200 LOC including the JSONC parser pass (reuse
`settings_json::parse_json_with_comments`).
