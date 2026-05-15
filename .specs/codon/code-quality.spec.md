---
id: REQ:codon/code-quality
type: requirement
status: draft
version: 0.0.2
level: SHOULD
summary: >
  Workspace-wide code-quality baseline for the codon crates: clippy
  stays clean, async I/O routes through the Fs trait, monolithic
  modules get decomposed, error paths surface to the user instead of
  being silently logged, and speculative abstractions are tracked
  until they earn a second consumer. v0.0.2 (phase 14) adds
  invariants the codebase had been documenting in CLAUDE.md without
  enforcing: no `unwrap()` / `expect()` in production codon code, no
  silent `let _ =`, modal/picker scaffolding shared, mode-tracker
  updates routed through a single bridge trait, `codon-keymap`
  decoupled from downstream codon crates, error patterns documented
  per crate, no silencer functions, `codon_bridge` exposing a
  single registry, naming consistent across crates, and docs
  verified at end-of-phase.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-5, TOPIC:topics/phase-14]
---

# Code quality baseline

## Context

The codon crates (`crates/*`, excluding `vendor/zed/`) accumulated a
small but consistent set of hygiene gaps as phase-2 → phase-5 features
landed. A pass with `cargo clippy --keep-going -p codon-mode -p
codon-keymap -p codon-session -p codon-agent -p codon-buffer -p
codon-command-palette -p codon-config -p codon-pickers -p file-manager
--all-targets` on the 2026-05-12 tree surfaced:

- 4 deny-level errors (`redundant_clone` × 3, `approx_constant` × 3
  for the literal `3.14` in codon-config tests/migrations) that abort
  the lint without `--keep-going`.
- ~14 warnings — mostly unused imports, unused function parameters,
  one derivable `Default` impl, and dead struct fields.
- A 1209-line `file_manager.rs` with four natural module seams.
- Three incomplete file-manager handlers (`create_file`,
  `create_directory`, `rename_entry`) that set `PendingInput` but
  never apply the result.
- `codon-config/src/writeback.rs` doing line-by-line string scanning
  instead of a TOML AST (`toml_edit`), making the splice fragile to
  TOML edge cases.
- `codon-config` async paths mixing `Arc<dyn Fs>` and `std::fs`,
  defeating the trait's mock-friendly seam.
- `codon-command-palette` completers silently swallowing errors via
  `.log_err().unwrap_or_default()` (modal.rs:522).
- `codon-buffer` trait shipped without a second consumer — the
  Helix-impl half of `REQ:codon/buffer-trait` is not yet in flight,
  so the trait is currently a one-impl forwarder.
- `crates/file-manager/` has zero unit tests; `crates/codon-config/`
  has tests but only for translation (no I/O / writeback coverage).

These are individually small. Grouped, they're load-bearing for two
reasons: (1) clippy-clean is a precondition for `script/clippy`-style
CI gating; (2) the writeback fragility and the silent-completer-error
behaviour will bite users before the design issues do.

## Why this is one REQ, not many

Most of these findings cross crate boundaries (the redundant-clone
pattern shows up in three crates; the unused-import pattern in four).
A per-crate spec would force the same rule to be restated five times.
The clauses here are the discipline; the per-area work happens in the
refining TASKs.

:::{requirement id="code-quality" level="SHOULD"}
The codon workspace SHOULD maintain:

- {#c-clippy-clean} `cargo clippy --all-targets -p codon-*
  -p file-manager` returns zero errors and zero warnings on the
  default lint set inherited from the workspace `[lints.clippy]`
  table. New deny-level lints (`redundant_clone`, `approx_constant`,
  `dbg_macro`, `todo`, `disallowed_methods`,
  `declare_interior_mutable_const`) MUST stay clean.
- {#c-fs-trait-purity} codon crates that accept `Arc<dyn fs::Fs>` for
  async I/O MUST NOT also reach into `std::fs` for the same paths —
  the trait is the seam that lets tests inject a fake. If a stdlib
  call is genuinely safe (sync init, well-known absolute path), the
  call site documents the reason in a one-line comment.
- {#c-module-decomposition} a single `*.rs` source file SHOULD stay
  under ~600 lines once it covers more than one concern (rendering,
  filesystem I/O, event dispatch, trait impls). When it crosses that
  line, the seams get extracted to sibling modules before the next
  feature lands.
- {#c-error-visibility} fallible user-facing operations (palette
  completers, file-manager filesystem ops, config writeback) MUST
  surface errors to the user via a toast / inline status / pane-local
  banner. `.log_err().unwrap_or_default()` is acceptable for
  diagnostic plumbing but not for paths the user is actively driving.
- {#c-speculative-abstractions} a trait with a single implementer and
  no current consumers MUST be tracked by an open task naming the
  expected second consumer and a defer/wontdo date if it does not
  materialize. The original target, `codon-buffer`, was marked
  wontdo on 2026-05-13 when Helix-as-engine integration was removed
  from the roadmap; the crate is slated for removal.
- {#c-test-coverage-floor} crates whose primary surface is a UI pane
  (file-manager, codon-pickers) SHOULD have at least one unit test
  per non-trivial pure function (sort/filter/state-transition logic).
  Rendering does not need coverage; logic does.
- {#c-no-unwrap-in-codon} no `unwrap()` / `expect()` outside test
  code in `apps/codon/src/` and `crates/codon-*` and
  `crates/file-manager/`, unless paired with a `// SAFETY: <invariant>`
  one-liner naming an invariant that is structurally enforced (not
  "should never happen"). Vendored-Zed boilerplate adapted under
  `apps/codon/src/zed*.rs` is exempt where the pattern matches
  upstream byte-for-byte; codon-added paths are not.
- {#c-no-silent-discards} no `let _ = <fallible expression>` outside
  test code. Either chain `.log_err()`, propagate with `?`, or
  document at the callsite in a one-line comment why the error is
  intentionally dropped. Parameter shadowing (`let _ = window;` to
  silence unused-warnings) does not count and is allowed.
- {#c-modal-scaffolding-shared} every codon modal and picker is
  constructed through a single shared scaffold (`codon-pickers`
  exports it; new home if cleaner). No hand-rolled `focus_handle` +
  `EventEmitter<DismissEvent>` + `set_command_active` triplet
  duplicated per crate. The scaffold wraps the codon-specific
  mode-tracker dance over Zed's existing `picker::Picker` and
  `workspace::ModalView` primitives.
- {#c-mode-dispatch-hook} `CodonModeTracker` updates flow through a
  single `PaneModeBridge` trait (in `codon-mode`). Each codon pane
  kind (terminal, file-manager, agent, jump, command-palette,
  cheatsheet) implements the trait; one central focus subscriber
  picks the active pane and dispatches. No crate updates the
  tracker directly via `cx.update_global` outside that bridge.
- {#c-keymap-decoupled} `crates/codon-keymap/Cargo.toml` does NOT
  depend on `codon-agent`, `codon-command-palette`, `codon-config`,
  `codon-jump`, or `codon-session`. Action registration lives in
  each owning crate's `init(cx)` function; keymap parses TOML and
  resolves actions through the GPUI action registry only.
- {#c-error-pattern-per-crate} each codon crate documents its error
  pattern (anyhow with `.context()` vs custom enum) in a one-line
  comment at the top of its `lib.rs`, and the rest of the crate
  follows it. Mixing within a single crate is the bug.
- {#c-no-silencer-functions} no `_silence_unused()` /
  `_assert_actions(_)` silencer functions in production code.
  Compile-time action-type assertions move to `#[cfg(test)]`
  modules; `#[allow(dead_code)]` markers either get removed (delete
  the dead item) or get a one-line comment naming the live consumer
  whose absence the allow tolerates.
- {#c-codon-bridge-single-registry} `workspace::codon_bridge`
  exposes one registry surface for codon-injected pane / panel
  kinds — not two. The function-pointer `OnceLock` and the
  closure `HashMap` patterns collapse into a single
  `pub fn codon_register_pane_kind(spec)` shape.
- {#c-naming-consistency} action namespaces follow
  `codon_<area>::*` (existing namespaces are grandfathered if
  renaming would break user keymaps; the rule applies to new
  actions). Per-crate enums are prefixed with the crate's
  vocabulary (`KeymapCheatTab`, not `CheatTab`). Lib-root source
  file in each codon crate matches the package name's underscore
  form (`codon_session.rs` for `codon-session`).
- {#c-doc-drift-check} CLAUDE.md and `codon-architecture.typ`
  claims are verified against current code at the end of phase 14.
  Mismatches get fixed in code or in docs — never left. The
  "no `unwrap()`" and "no silent `let _ =`" sentences in CLAUDE.md
  are removed unless `#c-no-unwrap-in-codon` and
  `#c-no-silent-discards` actually hold on the final tree.
- {#c-spec-lint-clean} `spec lint` returns zero errors on the
  codon `.specs/` tree. The 9 historical `R013` errors from
  phase-5-era renames either get placeholder spec files (option
  A), a `--since <hash>` cutoff in the spec-cli (option B), or
  documented acceptance in `.specs/AGENTS.md` (option C). The
  choice and rationale live in `.specs/AGENTS.md`.
:::

## Implementation

This is a discipline REQ, not a feature REQ — the implementation is
the set of refining TASKs in `.specs/phase-5/`. The TASKs split into
two buckets:

- **One-shot cleanup** — `clippy-baseline` resolves every diagnostic
  currently on the tree.
- **Targeted refactors** — `file-manager-decompose`,
  `file-manager-handler-commit`, `file-manager-tests`,
  `config-writeback-toml-edit`, `command-palette-error-surface`,
  `codon-buffer-second-consumer`.

Beyond phase-5, the gate moves to CI: a `cargo clippy` step that
fails the build on new diagnostics. That CI work is not in scope for
this REQ — it's a separate REQ:codon/ci-gates (not yet drafted).

### v0.0.2 implementation (phase 14)

The v0.0.2 clauses split into twelve TASKs under `.specs/phase-14/`:

- **One seam-introducing pair** (must land first because downstream
  TASKs depend on the seam): `modals-extract-scaffold`,
  `mode-bridge-trait`.
- **Two sweeping cleanups** that work file-by-file once the seams are
  in: `hygiene-kill-unwraps`, `hygiene-kill-silent-discards`.
- **Two structural moves**: `keymap-decouple`,
  `codon-bridge-single-registry`.
- **Four targeted clean-ups**: `error-pattern-per-crate`,
  `dead-code-purge`, `naming-consistency-sweep`,
  `test-coverage-floor` (continuation of v0.0.1 ground).
- **Two end-of-phase passes**: `doc-drift-resolve`,
  `spec-lint-stale-refs`.

Each TASK is one branch / one merge-from-worktree, with a
`Spec-Ref:` commit trailer naming the clause it implements.

## Out of scope

- `vendor/zed/` lints — Zed has its own `./script/clippy` discipline
  documented in `vendor/zed/CLAUDE.md`. The codon-config "stdfs vs
  Arc<dyn Fs>" rule does not apply to vendored Zed code.
- Style-only nits (`needless_return`, `single_match`, etc.) — the
  workspace lint table already sets `style = "allow"`, intentionally.
  Don't bring them back without a separate discussion.
- Performance lints — covered separately when a profile says they
  matter; not part of the clippy-clean baseline.
