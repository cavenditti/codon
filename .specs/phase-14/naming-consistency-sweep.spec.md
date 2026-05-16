---
id: TASK:phase-14/naming-consistency-sweep
type: task
status: draft
version: 0.0.1
summary: >
  Rename per-crate enums to be crate-prefixed (`CheatTab` →
  `KeymapCheatTab`); document the `codon_<area>::*` action
  namespace convention in CLAUDE.md (do not rename existing
  actions if it would break user keymaps); verify each codon
  crate's lib-root source file matches the underscored package
  name.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-naming-consistency
---

# Naming consistency sweep

## What changes

Three small categories:

1. **Crate-internal enum prefixes.** Some enums read ambiguously
   when seen at a callsite outside their defining file:
   - `CheatTab`, `CheatMode` in
     `crates/codon-keymap/src/cheatsheet_modal.rs` →
     `KeymapCheatTab`, `KeymapCheatMode`.
   - Any other enum exported from a codon crate that doesn't carry
     the crate's vocabulary in its name — sweep and rename.

2. **Action namespace convention.** Actions today use a mix:
   `CodonPalette::toggle` vs `codon_session::*` vs
   `codon_agent::Explain`. The CONVENTION going forward is
   `codon_<area>::<Verb>` (matching the GPUI macro form). DO NOT
   rename existing actions — they're load-bearing in user keymaps
   that anyone has typed by hand. INSTEAD, document the convention
   in CLAUDE.md and apply it to new actions.

3. **Lib-root source files.** The Cargo package is `codon-session`
   (hyphen); the lib-root file is `crates/codon-session/src/codon_session.rs`
   (underscore — matches Cargo convention). Confirm every codon crate
   follows this. The audit flagged `codon-session` as a possible
   inconsistency, but check; the file may already be `lib.rs` or
   `codon_session.rs` correctly.

## Approach

1. **Enum rename pass.** Use rust-analyzer / IDE rename for each
   identified enum. Confirm callsites all update; `cargo build` clean.
   Single commit per crate.

2. **CLAUDE.md update.** Append a "Naming conventions" subsection
   to the "Workflow conventions" section, documenting:
   - Action names: `codon_<area>::<Verb>` for new actions; existing
     names grandfathered.
   - Enum names: crate-prefixed when the type leaves the crate
     (`KeymapCheatTab`, not `CheatTab`).
   - Lib-root file: underscored package name (`codon_session.rs`).

3. **Lib-root audit.** For each `crates/codon-*/Cargo.toml`,
   check the `[lib]` section or default. Confirm the source file
   name matches. If `lib.rs` is in use for any crate, that's also
   fine — the rule is "consistent within the workspace"; pick one.
   Recommendation: stick with the current `codon_<area>.rs`
   convention since several crates already use it.

## Non-goals

- Not renaming any GPUI action. The action registry is keyed on
  the typed name; renames break user keymaps.
- Not introducing a `prelude` module per crate. Out of scope.

## Files touched

- `crates/codon-keymap/src/cheatsheet_modal.rs` — enum renames.
- Other crates flagged by the enum sweep (search:
  `rg -n 'enum [A-Z][a-zA-Z]*\b' crates/codon-*/src/**.rs | rg -v 'fn |//'`
  and judge each).
- `CLAUDE.md` — new "Naming conventions" subsection.
- Any crate whose lib-root file doesn't match — rename via
  `git mv` + Cargo.toml `[lib] path` adjustment if needed.

## Verification

- `cargo build -p codon` — clean.
- `git grep -nE '^pub (struct|enum) (Cheat[A-Z]|[A-Z]\w*Modal\b)' crates/codon-*`
  shows every cross-crate exported type has its crate vocabulary in
  the name.
- CLAUDE.md "Naming conventions" subsection exists.
