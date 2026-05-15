---
id: TASK:phase-14/error-pattern-per-crate
type: task
status: draft
version: 0.0.1
summary: >
  Each codon crate's `lib.rs` documents its error pattern (anyhow
  vs custom enum) in a one-line preamble, and the rest of the
  crate follows it. Fix the mixed cases: `codon-session::registry`
  defines a custom enum but only some functions use it;
  `codon-command-palette/src/completer.rs` mixes `anyhow::Result`
  with `.unwrap()` in the same module; `file-manager/` has no
  declared pattern.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-error-pattern-per-crate
---

# One error pattern per crate, documented

## What changes

Today codon crates handle errors three ways with no convention:

- `codon-config` uses `anyhow::Result` with `.context()` everywhere
  (clean — the model).
- `codon-session/src/registry.rs:16` defines
  `SessionRegistryError` (a custom enum) but only 3 of 7 functions
  in that crate use it; the rest bail with `?` on anyhow.
- `codon-command-palette/src/completer.rs` mixes `anyhow::Result`
  with `.unwrap()` in the same file.
- `file-manager/src/` is the largest crate and has no declared
  pattern; the audit found a mix of `anyhow::Result`, custom
  errors, and silent failures.

## Approach

For each crate, in this order:

1. **codon-config** — already clean. Add the one-line preamble:
   ```rust
   //! Error pattern: `anyhow::Result` with `.context()` at every
   //! `?` boundary. No custom error types.
   ```

2. **codon-session** — decide: keep `SessionRegistryError` as a
   typed boundary error, or remove it in favour of anyhow.
   Recommendation: keep it for the registry layer only (the
   public KVP-backed API), use anyhow for everything else
   (overview, picker, actions). Add the preamble:
   ```rust
   //! Error pattern: `SessionRegistryError` at the `registry`
   //! module's public API (boundary errors callers can match on).
   //! `anyhow::Result` everywhere else.
   ```
   Audit each function in the crate and align.

3. **codon-command-palette** — pick `anyhow::Result`. Replace
   any `.unwrap()` in `completer.rs` per the
   `hygiene-kill-unwraps` TASK. Add the preamble.

4. **file-manager** — pick `anyhow::Result` with toasts for
   user-driven failures (already the dominant pattern). Add the
   preamble. Audit per-module and align.

5. **codon-agent, codon-jump, codon-keymap, codon-pickers,
   codon-mode, codon-panes** — audit, declare, align. Most
   will be `anyhow::Result`; the preamble is the deliverable
   even when no code changes.

## Non-goals

- Not introducing a new `thiserror` dependency. `anyhow` for
  internals + hand-rolled `enum` at API boundaries is the
  established pattern.
- Not changing the public signature of `SessionRegistryError`
  variants. If a variant needs to be added, that's a separate
  decision.

## Files touched

- Every codon crate's `src/lib.rs` (or `src/codon_session.rs` etc.)
  gets a one-line preamble.
- `crates/codon-session/src/registry.rs` and adjacent files —
  align to the declared pattern.
- `crates/codon-command-palette/src/completer.rs` — align.
- `crates/file-manager/src/*` — audit + align where mixed.

## Verification

- `cargo build` — clean.
- `rg -n '^//! Error pattern:' crates/codon-* crates/file-manager`
  returns one hit per crate's lib root.
- For each crate, the dominant error-return type in `pub fn`
  signatures matches what the preamble declares.
