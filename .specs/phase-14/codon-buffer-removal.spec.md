---
id: TASK:phase-14/codon-buffer-removal
type: task
status: draft
version: 0.0.1
summary: >
  Delete `crates/codon-buffer/`, remove the workspace member
  entry, and remove the workspace-deps `codon-buffer.workspace`
  line. The crate has zero consumers since
  `REQ:codon/buffer-trait` was superseded by the decision to
  adopt Zed's vim + `helix_default` wholesale.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-no-silencer-functions
---

# Remove the superseded `codon-buffer` crate

## What changes

`REQ:codon/buffer-trait` is `status: superseded` — codon does not
plug `helix_view::Document` in as an alternate buffer backend, and
no second consumer of a buffer trait is planned. The crate at
`crates/codon-buffer/` exists from the original Phase 4 prototype
(the trait definition + a `language::Buffer` impl) but has zero
runtime consumers. Leaving it in the workspace is dead surface —
new contributors find it, read it, and waste time reasoning about
which crate to depend on.

## Approach

1. `rg -n 'codon[-_]buffer'` across the repo. Confirm zero
   non-self references. Document the count in the commit body
   for future grep.
2. `cargo build -p codon` — establish a clean baseline.
3. Delete `crates/codon-buffer/` entirely.
4. Remove the `crates/codon-buffer` line from the workspace
   `Cargo.toml` `members` array.
5. Remove the `codon-buffer.workspace = true` entry from the
   `[workspace.dependencies]` block.
6. `cargo build -p codon` again — must remain clean.
7. `( cd vendor/zed && ./script/clippy )` — confirm vendored Zed
   is unaffected (it shouldn't reference codon-buffer; verify).
8. Update the codon design doc (`codon-architecture.typ`) and
   CLAUDE.md if either still mentions `codon-buffer` as a live
   crate. (As of v0.4 of the design doc the crate is mentioned
   only in the "Cleanup pending" section — that bullet stays
   until this task lands, then comes out.)

## Non-goals

- Not deleting the `REQ:codon/buffer-trait` spec entry. It stays
  as `status: superseded` for historical traceability — the
  rationale lives in the REQ, not in a commit message.
- Not touching the `vendor/helix/` submodule. Helix stays
  vendored as *reference material* for how Helix implements
  features we want to mirror.

## Verification

- `cargo build -p codon` clean.
- `cargo test --workspace` clean (no test depends on the deleted
  crate).
- `rg -n 'codon[-_]buffer'` returns zero hits.
- The codon design doc's "Cleanup pending" section no longer
  lists the codon-buffer removal.

## Files touched

- `crates/codon-buffer/` — deleted (whole directory).
- `Cargo.toml` (workspace root) — member entry + workspace-deps
  entry removed.
- `codon-architecture.typ` — "Cleanup pending" bullet removed.
- `CLAUDE.md` — none expected; verify and update if a
  `codon-buffer` reference exists.
