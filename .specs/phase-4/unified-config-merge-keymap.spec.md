---
id: TASK:phase-4/unified-config-merge-keymap
type: task
status: accepted
version: 0.0.1
summary: >
  Point codon-keymap at ~/.config/codon/codon.toml's [bindings.*]
  sub-tree so keymap and settings share one file.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/unified-config#c-merge-keymap
---

# codon-keymap reads the unified file

## What ships

`crates/codon-keymap/src/keymap.rs::load_codon_keymap` gains a second
load path: `~/.config/codon/codon.toml`. The file's `[bindings.*]`
sub-tree matches the existing keymap.toml format byte-for-byte.

Loading order:

1. Apply the embedded default keymap (unchanged).
2. If `codon.toml` exists, parse it and apply `[bindings.*]`.
3. Else if the legacy `~/.config/codon/keymap.toml` exists, parse it
   and apply (back-compat for users who haven't migrated).

After the migration task lands, step 3 logs a deprecation hint.

## Files

- [`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
  — refactor `load_codon_keymap` to call a small helper that picks
  the right path. Keep `parse_keymap` unchanged (the TOML shape is
  the same, just embedded under a parent key).

## Tests

- Fixture `codon.toml` with both `[settings.*]` and `[bindings.*]`
  → bindings load correctly, settings are passed through to
  `codon-config` (which is responsible for them).

Small. ~80 LOC.
