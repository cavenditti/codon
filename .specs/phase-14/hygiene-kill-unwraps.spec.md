---
id: TASK:phase-14/hygiene-kill-unwraps
type: task
status: draft
version: 0.0.1
summary: >
  Eliminate every `unwrap()` / `expect()` in production codon code
  (apps/codon/src and crates/codon-* and crates/file-manager),
  replacing with `?` propagation, `.context()`, defensive defaults,
  or a `// SAFETY:` block where the invariant is structurally
  enforced. Vendored-Zed-adapted boilerplate that matches upstream
  byte-for-byte is exempt.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-no-unwrap-in-codon
---

# Kill `unwrap()` / `expect()` in production codon code

## What changes

A grep on the current tree:

```
$ rg -n '\.unwrap\(\)|\.expect\(' crates/codon-* crates/file-manager apps/codon/src \
    | grep -v '#\[cfg(test)\]' \
    | awk -F: '{print $1}' | sort | uniq -c | sort -rn | head
325 apps/codon/src/zed.rs
 57 crates/file-manager/src/file_manager.rs
 46 apps/codon/src/zed/open_listener.rs
 11 apps/codon/src/main.rs
  8 crates/codon-jump/src/codon_jump.rs
  7 crates/file-manager/src/search.rs
  ...
```

Most of `apps/codon/src/zed.rs` and `apps/codon/src/zed/*` are
direct adaptations of vendored Zed's `crates/zed/src/main.rs`.
Those are exempt **only** where the line matches the upstream byte
for byte (or where it does in spirit — the diff is a struct rename,
not a logic change). The TASK author runs a side-by-side check
against `vendor/zed/crates/zed/src/main.rs` and flags codon-added
lines for fixing.

## Priority targets (codon-added, NOT vendored-adapted)

These are confirmed codon-added paths from the audit. Each is one
edit (or one small group):

- `apps/codon/src/zed.rs:412–513` — codon keymap load region. ~10
  `unwrap()` calls on the embedded TOML / user keymap parse paths.
  Convert to `anyhow::Result<()>` with `.context("loading codon keymap")`
  and propagate up to startup. On failure, log and fall back to the
  embedded default (do not crash codon over a malformed user keymap).
- `apps/codon/src/zed.rs:1094` — `.downcast::<MultiWorkspace>().unwrap()`.
  Defensive: log and early-return if the downcast fails; the call site
  is a window-dispatch hook where a None is recoverable.
- `apps/codon/src/reliability.rs:164–165` — `STARTUP_TIME.get().unwrap()`
  and `hang_time.unwrap()`. Defensive defaults: `STARTUP_TIME` should
  be wrapped in `OnceLock` access that logs-and-defaults; hang_time
  guarded by `if let Some`.
- `crates/codon-session/src/overview.rs:60, 69, 77, 84` — `.expect()`
  on layout capture. The capture path already returns `Result` deeper
  down; thread the error up and toast on failure.
- `crates/codon-session/src/break_pane.rs:180, 189` — `.unwrap()` on
  pane manipulation. Same approach as overview.
- `crates/codon-session/src/actions.rs` — audit and fix (see
  `hygiene-kill-silent-discards` for the `let _ =` half).
- `crates/codon-jump/src/codon_jump.rs` — 8 unwraps; audit per-callsite,
  most are likely `Option::unwrap` on infallible chains; convert to
  `if let Some` or `.expect("<SAFETY: invariant>")`.
- `crates/file-manager/src/file_manager.rs` — 57 hits but many are
  inside `#[cfg(test)]` modules. Audit non-test ones; the audit
  flagged the public file-op handlers as the highest risk.

## Approach

1. Build a worktree (`git worktree add ../codon-phase14-unwraps -b worktree-hygiene-unwraps`).
2. File-by-file:
   - Run the grep restricted to that file (excluding test modules).
   - For each hit, classify: vendored-adapted / codon-added /
     genuinely-infallible.
   - Vendored-adapted: leave; add a one-line note above explaining
     it mirrors upstream if the call is the kind of thing the lint
     would catch.
   - Codon-added fallible: convert to `?` + `.context()`, or
     defensive default + `.log_err()`.
   - Genuinely infallible (e.g., `regex::Regex::new(static_str).unwrap()`
     on a compile-time-constant pattern): add a `// SAFETY: <reason>`
     comment above the line. The reason names the invariant
     concretely.
3. Build after each file: `cargo build -p <crate>`.
4. Re-grep at the end; the only remaining hits must be either
   `#[cfg(test)]`, vendored-adapted, or paired with a `// SAFETY:`
   line.

## Non-goals

- Not refactoring the public API of `codon-session` or any other
  crate to make error types propagate. If `.unwrap()` removal needs
  a `Result<()>` signature change, that's in scope; if it needs a
  new enum variant in `SessionRegistryError`, defer that to
  `error-pattern-per-crate`.
- Not touching `vendor/zed/` source files. They have their own
  rules in `vendor/zed/CLAUDE.md` and a separate clippy gate.

## Files touched

A pass across `apps/codon/src/**` and `crates/codon-*/src/**` and
`crates/file-manager/src/**`. The largest single file is
`apps/codon/src/zed.rs`; the most impactful single block is the
keymap-load region at 412–513.

## Verification

- `cargo build -p codon` — clean.
- `cargo clippy --all-targets -p codon-* -p file-manager -p codon` —
  clean. The workspace `[lints.clippy]` already denies `unwrap_used`
  / `expect_used` for some crates; reconcile if it doesn't already.
- ```
  rg -n '\.unwrap\(\)|\.expect\(' \
    crates/codon-* crates/file-manager apps/codon/src \
    | rg -v '#\[cfg\(test\)\]|tests/|// SAFETY:'
  ```
  Returns zero hits, OR every remaining hit is documented as
  vendored-Zed-adapted boilerplate (one-line comment above).
- Manual smoke: malformed `~/.config/codon/keymap.toml` is loaded;
  codon launches with a logged error and the embedded default
  keymap, instead of crashing.
