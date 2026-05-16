---
id: TASK:phase-14/doc-drift-resolve
type: task
status: draft
version: 0.0.1
summary: >
  End-of-phase pass: verify CLAUDE.md, codon-architecture.typ, and
  assets/config/keymap.example.toml against the now-cleaner code.
  Mismatches get fixed in code or in docs — never left.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-doc-drift-check
---

# Doc drift resolution

## What changes

Three documents need a spot-check at the end of phase 14, after
the other TASKs have landed:

1. **CLAUDE.md (project root)** — currently states "no `unwrap()`"
   and "no silent `let _ =`" as repo-wide rules. After
   `hygiene-kill-unwraps` and `hygiene-kill-silent-discards` land,
   the rule either holds (keep the sentence) or doesn't (rewrite
   it). Also verify:
   - The "Modal layer" architecture paragraph still matches reality
     after `mode-bridge-trait` lands.
   - The "Sessions + windows" paragraph still matches after any
     codon-bridge changes from `codon-bridge-single-registry`.
   - The "Keymap" paragraph still matches after `keymap-decouple`.

2. **codon-architecture.typ (project root)** — the long-form
   design doc. Spot-check the module-layout and data-flow
   diagrams against current crate structure. Anything that
   references the old keymap dependency tree or the old
   codon_bridge registries needs updating.

3. **assets/config/keymap.example.toml** — confirm it still
   matches the embedded default TOML in
   `crates/codon-keymap/src/keymap.rs`. Any new bindings added in
   intermediate phases (esp. phase-13 status-bar work) must be
   reflected.

## Approach

This is a pure reading + editing TASK. It runs LAST in the phase.

1. Run `cargo build -p codon` — confirm clean.
2. Run the verification greps from `hygiene-kill-unwraps` and
   `hygiene-kill-silent-discards` — confirm zero hits.
3. Read CLAUDE.md top to bottom; for each claim, ask "does this
   match the code as of right now?" Fix or remove mismatches.
4. Read codon-architecture.typ section by section; same question.
5. Diff `assets/config/keymap.example.toml` against the embedded
   default — make them match.
6. Update the `Recent commits` block in CLAUDE.md (if any) to
   reflect phase-14 work.

## Non-goals

- Not rewriting CLAUDE.md from scratch.
- Not introducing new documentation files. Edits only.
- Not updating `.specs/` files — those are spec docs, separate
  from project docs.

## Files touched

- `CLAUDE.md` — surgical edits to claims that drift.
- `codon-architecture.typ` — surgical edits to claims that drift.
- `assets/config/keymap.example.toml` — align with embedded
  default.

## Verification

- Manual: read each of the three documents; every claim is
  accurate as of the current tree.
- `diff <(grep -E '^\s*[a-z_]+ =' assets/config/keymap.example.toml | sort)
        <(rust-script to extract the embedded TOML default | sort)`
  shows no missing bindings (or the diff is the documented
  user-template/embedded-default split).
