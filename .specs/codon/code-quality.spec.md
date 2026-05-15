---
id: REQ:codon/code-quality
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Workspace-wide code-quality baseline for the codon crates: clippy
  stays clean, async I/O routes through the Fs trait, monolithic
  modules get decomposed, error paths surface to the user instead of
  being silently logged, and speculative abstractions are tracked
  until they earn a second consumer.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-5]
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
- {#c-modal-scaffolding-shared} the boilerplate every codon modal
  repeats — owning a `FocusHandle`, implementing `Focusable`,
  implementing `EventEmitter<DismissEvent>`, and toggling
  `CodonModeTracker.command_active` while open — SHOULD be expressed
  once as a shared `ModalScaffold` builder in `codon-pickers`, and
  each modal SHOULD hold an instance of it by composition rather than
  re-implementing the dance inline. Modals that do not change the
  global mode indicator declare that explicitly via an `Inert` tag so
  the choice stays visible at the callsite.
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

## Out of scope

- `vendor/zed/` lints — Zed has its own `./script/clippy` discipline
  documented in `vendor/zed/CLAUDE.md`. The codon-config "stdfs vs
  Arc<dyn Fs>" rule does not apply to vendored Zed code.
- Style-only nits (`needless_return`, `single_match`, etc.) — the
  workspace lint table already sets `style = "allow"`, intentionally.
  Don't bring them back without a separate discussion.
- Performance lints — covered separately when a profile says they
  matter; not part of the clippy-clean baseline.
