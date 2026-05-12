---
id: TASK:phase-5/clippy-baseline
type: task
status: accepted
version: 0.1.0
summary: >
  Resolve every clippy diagnostic currently on the codon crates so
  `cargo clippy --all-targets` returns zero errors and zero warnings
  for the codon-* + file-manager package set. Shipped 2026-05-12
  across five commits (codon-config / codon-pickers / codon-mode /
  codon-command-palette / file-manager) plus a mop-up commit for
  codon-session and apps/codon.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-clippy-clean
---

# Clippy baseline — get to zero

## What ships

A single pass over the codon-specific crates that lands the codebase
at zero clippy diagnostics on the workspace default lint set. The
diagnostics observed on the 2026-05-12 tree, grouped:

### Deny-level errors (must fix in source)

- [`crates/codon-config/src/codon_config.rs:148`](spec:src:crates/codon-config/src/codon_config.rs)
  — `redundant_clone` on `path.clone()`; the value is dropped right
  after the call. Remove the clone.
- [`crates/codon-config/src/migrate.rs:240`](spec:src:crates/codon-config/src/migrate.rs)
  and `:248`,
  [`crates/codon-config/src/toml_to_json.rs:58`](spec:src:crates/codon-config/src/toml_to_json.rs)
  — `approx_constant` on the literal `3.14` in TOML test fixtures.
  These are test sentinels, not magic numbers. Replace with `3.5`
  (or any other float that is not within `f64::EPSILON` of π) and
  drop a one-line comment that it's an arbitrary test value, *or*
  add a scoped `#[allow(clippy::approx_constant)]` with the same
  comment. Prefer the literal swap — keeps the lint live elsewhere.
- [`crates/codon-pickers/src/open_rewire.rs:38`](spec:src:crates/codon-pickers/src/open_rewire.rs)
  — `redundant_clone` on `weak.clone()`. Remove.
- [`crates/file-manager/src/file_manager.rs:300`](spec:src:crates/file-manager/src/file_manager.rs)
  — `redundant_clone` on `entry.path.clone()` (the value is moved
  into the closure immediately). Remove.

### Warnings (must fix to reach zero)

- Unused imports: `AppContext as _` in
  `codon-config/src/codon_config.rs:21`,
  `AppContext` in `codon-pickers/src/open_rewire.rs:3` and
  `codon-pickers/src/dir_picker.rs` (top of file),
  `StyledExt` and `fuzzy::StringMatchCandidate` and `AppContext` in
  `file-manager/src/file_manager.rs:11–14`,
  `fuzzy::StringMatchCandidate` + `AppContext as _` in
  `codon-command-palette/src/completer.rs:25–26`.
  All five are leftovers from refactors — drop them.
- `codon-pickers/src/dir_picker.rs:115` — `consider using sort_by_key`
  on the case-insensitive name compare. Replace
  `sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()))`
  with `sort_by_key(|c| c.name.to_lowercase())`.
- `codon-mode/src/selection.rs:21` — `manual_impl_default`; the
  enum derives `Clone, Debug` already. Add `#[derive(Default)]` on
  the enum with `#[default]` on the variant the manual impl returns,
  then delete the manual block.
- Unused `window: &mut Window` parameters in
  `file-manager/src/file_manager.rs:380, 386, 392, 859`. All four
  are genuinely unused: the three setup handlers (`create_file`,
  `create_directory`, `rename_entry`) only queue `PendingInput`
  state, and the actual commit phase lives in `handle_insert_key`
  which uses `window` directly. Prefix with `_window` at all four
  sites — the canonical clippy fix for callback signatures whose
  arity is fixed by the contract.
- Dead fields `is_hidden` and `size` on the file-manager `DirEntry`
  struct (around `file_manager.rs:44`). Both are stubs for planned
  feature wire-ups, not deletions. Land alongside the clippy fix:
  `is_hidden` → render-time dim of hidden rows when `show_hidden =
  true`; `size` → focused-entry size in the status bar (with item
  count for directories using the existing preview). The dead-field
  warnings clear without `#[allow]` once the fields are read. The
  read-time filter in `read_dir_sync` stays as-is — when
  `show_hidden = false`, hidden entries never reach the vec, and
  the dim branch never fires. See the
  TASK:phase-5/file-manager-fs-trait-create-rename sibling for the
  related FS-trait migration of create/rename. `is_hidden` is set at
  read time but never read afterwards (filtering happens before
  construction) — wire it through the `.` toggle or remove. `size`
  is never set or read — remove it and re-add when sort-by-size is
  in scope.

## Acceptance

- `cargo clippy --all-targets -p codon-mode -p codon-keymap -p
  codon-session -p codon-agent -p codon-buffer -p
  codon-command-palette -p codon-config -p codon-pickers -p
  file-manager` returns zero diagnostics, *without* `--keep-going`.
- No `#[allow(...)]` attributes added except the deliberately-scoped
  `approx_constant` suppression in test fixtures (and only if the
  literal-swap path is rejected during review).
- The `_window` rename happens only at the one true-positive site
  (line 859). The other three `window` params stay as named bindings
  because the handler-commit task will use them.

## Out of scope

- The handler-commit work itself (TASK:phase-5/file-manager-handler-commit).
- The decomposition of `file_manager.rs` into modules
  (TASK:phase-5/file-manager-decompose).
- Any new `#[deny]` additions to the workspace lint table — that's a
  follow-up once we're at zero and want to keep it there.

Effort: small. ~30 LOC of deletions, ~10 LOC of import cleanups.
The `is_hidden` / `size` decision is the only judgement call.
