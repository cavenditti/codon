---
id: TASK:phase-14/dead-code-purge
type: task
status: draft
version: 0.0.1
summary: >
  Remove silencer functions and stale `#[allow(dead_code)]`
  markers in codon crates. Move compile-time action-type
  assertions into `#[cfg(test)]` modules; delete `_silence_unused`
  / `mock_provider_unused_warning_silenced` style helpers.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-no-silencer-functions
---

# Purge silencer functions and stale dead-code markers

## What changes

The audit flagged these silencer / assertion helpers in production
code:

- `crates/codon-session/src/picker.rs:224–225` —
  `fn _assert_actions(_: &actions::SessionSwitch)` exists only to
  silence an unused-action warning at compile time. Move to a
  `#[cfg(test)] mod compile_assertions { ... }` block in the same
  file.
- `crates/codon-session/src/window_rename.rs:137–138` — same
  pattern.
- `crates/codon-panes/src/peek.rs:120–121` —
  `fn _silence_unused()` with no body purpose. Delete; if the
  silenced item is genuinely unused, delete it instead.
- `crates/codon-jump/src/codon_jump.rs:1583–1585` —
  `fn mock_provider_unused_warning_silenced()`. Same — delete the
  silencer and either delete the silenced item or move it under
  `#[cfg(test)]`.
- `crates/codon-jump/src/codon_jump.rs:553` —
  `#[allow(dead_code)]` on `workspace_subscription: Option<Subscription>`.
  Check whether the field is actually live (it holds a
  subscription's lifetime, which can look dead to clippy). If live,
  replace the `#[allow]` with a one-line `// keeps subscription alive`
  comment. If genuinely unused, delete the field.

## Approach

For each callsite:

1. Search for the symbol being silenced.
2. If the symbol IS used at runtime: the silencer is unnecessary —
   delete it and verify the warning doesn't reappear. (It may have
   been added during a refactor that has since been undone.)
3. If the symbol is ONLY used for compile-time type assertion:
   move it into `#[cfg(test)] mod compile_assertions` in the same
   file, with a short comment explaining the assertion's purpose.
4. If the symbol is genuinely dead: delete it entirely (the
   silencer goes with it).

## Audit other `#[allow(dead_code)]` markers

After the named callsites, run a global sweep:

```
rg -n '#\[allow\(dead_code\)\]' crates/codon-* crates/file-manager apps/codon
```

For each hit, apply the same three-way classification: live (replace
with explanatory comment), test-only (move to cfg-test), or dead
(delete).

## Non-goals

- Not refactoring the surrounding code. Each fix should be
  ≤ 10 lines.
- Not changing the public API of any crate.

## Files touched

- `crates/codon-session/src/picker.rs`
- `crates/codon-session/src/window_rename.rs`
- `crates/codon-panes/src/peek.rs`
- `crates/codon-jump/src/codon_jump.rs`
- Any other file flagged by the `#[allow(dead_code)]` sweep.

## Verification

- `cargo build -p codon` — clean (no warnings).
- `rg -n '_silence_unused|_assert_actions|mock_provider_unused' crates/`
  returns hits only inside `#[cfg(test)]` modules.
- `rg -n '#\[allow\(dead_code\)\]' crates/codon-* crates/file-manager apps/codon`
  returns zero hits OR every hit is paired with a one-line
  explanatory comment.
