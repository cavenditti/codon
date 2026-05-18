---
id: TASK:phase-15/keymap-prefix-configurable
type: task
status: draft
version: 0.0.1
summary: >
  Replace the hard-coded `cmd-k` prefix in `DEFAULT_KEYMAP` with a
  `"prefix"` sentinel token, add a `[keymap] prefix = "..."`
  setting to `codon.toml`, and have the loader substitute the
  configured prefix before binding. Default stays `cmd-k` for
  backward compatibility.
owners: [carlo]
progress: done
refines:
  - REQ:codon/keymap#c-prefix-configurable
---

# Configurable chord prefix

## What changes

[`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
holds an embedded `DEFAULT_KEYMAP: &str` whose `[bindings.global]`
section uses `"cmd-k <X>"` for the entire chord-prefix family
(window lifecycle, sessions, splits, panes-from-panels, agent
verbs, jumps, palette, cheatsheet — ~40 chords). The token must
become a sentinel:

```diff
-"cmd-k c"   = "codon_session::WindowNew"
-"cmd-k n"   = "codon_session::WindowNext"
-"cmd-k s s" = "codon_session::SessionSwitch"
+"prefix c"   = "codon_session::WindowNew"
+"prefix n"   = "codon_session::WindowNext"
+"prefix s s" = "codon_session::SessionSwitch"
```

A new optional `[keymap]` table in `codon.toml` carries the
override:

```toml
[keymap]
prefix = "ctrl-x"
```

Loader changes in `keymap.rs`:

- Extend `CodonKeymap` with `keymap: Option<KeymapTopLevel>` where
  `KeymapTopLevel { prefix: Option<String> }`.
- `load_codon_keymap` resolves the prefix in this order: user
  `codon.toml` → legacy `keymap.toml` → fallback `"cmd-k"`.
- After `parse_keymap` produces `Vec<(String, String, Option<String>)>`
  for both the embedded defaults and the user file, walk each
  keystroke and substitute a leading `"prefix"` token with the
  resolved prefix string before calling `build_binding`. Examples:
  `"prefix c"` → `"ctrl-x c"`, `"prefix shift-w n"` →
  `"ctrl-x shift-w n"`, `"prefix \\"` → `"ctrl-x \\"`.
- Only the *leading* `prefix` token is substituted — a chord like
  `"alt-prefix"` (unlikely, but possible) passes through unchanged.
- Reuse the existing reload path
  ([`codon-keymap::reload_keymap`](spec:src:crates/codon-keymap/src/keymap.rs))
  so a saved `codon.toml` swap of `prefix` rebinds on next load
  without a process restart.

Update sites:

- `crates/codon-keymap/src/keymap.rs` — `DEFAULT_KEYMAP`, the
  `CodonKeymap`/`parse_keymap` shape, `load_codon_keymap`,
  `codon_default_bindings`/`codon_user_bindings`,
  and a new prefix-resolution helper.
- `assets/config/codon.example.toml` — document the
  `[keymap] prefix` setting and update inline references from
  `cmd-k` to `prefix` where they describe the chord family.
- `CLAUDE.md` — rewrite the "Keymap." paragraph to describe the
  configurable prefix.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — the cheatsheet
  reads `codon_default_bindings()` / `codon_user_bindings()`; verify
  that expanded chords render correctly (they should; the cheatsheet
  shows whatever `build_binding` consumed). No behavior change
  expected, but add a smoke test.

Tests (`crates/codon-keymap/src/tests.rs` or a new `tests/` file
beside `keymap.rs`):

- `prefix_default_substitutes_cmd_k` — with no user override,
  `"prefix c"` in defaults binds as `"cmd-k c"`.
- `prefix_override_rekeys_defaults` — with
  `[keymap] prefix = "ctrl-x"`, the same chord binds as `"ctrl-x c"`.
- `prefix_substitutes_in_user_bindings` — a user-defined
  `"prefix t"` binding expands using the same prefix.
- `non_prefix_chord_unchanged` — `"cmd-shift-s"` and `"ctrl-l"`
  pass through untouched.
- `prefix_reload_drops_old_chord` — after a reload with a new
  prefix, the previous prefix's chords are not present in
  `codon_default_bindings()` (sanity check on rebinding atomicity).

## Why this clause

The user is migrating from a tmux config keyed on `ctrl-x` and the
current user-config layer can only add — not unbind — defaults.
Mirror blocks in user TOML leave both prefixes active. Pushing the
prefix into a setting is the smallest surface that genuinely lets
one prefix replace the other, and it stays additive: existing
installs see no change because the default resolves to `"cmd-k"`.

## Done when

- `DEFAULT_KEYMAP` contains zero literal `"cmd-k "` keystroke
  prefixes; every previous `cmd-k` chord uses the `prefix`
  sentinel.
- A user `codon.toml` with `[keymap] prefix = "ctrl-x"` binds the
  full chord family under `ctrl-x` and binds nothing under
  `cmd-k` from the defaults.
- The five tests above pass.
- `vendor/zed/script/clippy` (or `cargo clippy -p codon-keymap`)
  reports no new warnings.
- `spec lint` reports zero errors.
- `assets/config/codon.example.toml` and `CLAUDE.md` are updated;
  no stale `cmd-k`-as-prefix wording remains.
