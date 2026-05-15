---
id: TASK:phase-14/test-coverage-floor
type: task
status: draft
version: 0.0.1
summary: >
  Raise the test-coverage floor in codon crates that currently
  have none — codon-agent, codon-pickers, codon-panes,
  codon-session/actions, and the cheatsheet modal in codon-keymap.
  At least one unit test per non-trivial pure function per the
  v0.0.1 `#c-test-coverage-floor` clause.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-test-coverage-floor
---

# Test-coverage floor for the currently-untested codon crates

## What changes

Crates whose primary surface is a UI pane SHOULD have at least one
unit test per non-trivial pure function (v0.0.1 clause). The
phase-5 TASK `file-manager-tests` established the bar for the
file-manager crate. This TASK extends it to the crates that fell
through the cracks:

- **codon-agent** — zero `#[cfg(test)]` modules. The cross-pane
  agent verbs (`Explain`, `Summarize`, `Refactor`) take a selection
  and seed an `AgentPanel` call. Add tests for the seed-construction
  pure logic (selection text → prompt template).
- **codon-pickers** — zero tests. Once `modals-extract-scaffold`
  lands, that's the obvious test target. In the meantime, test
  the existing `DirPicker` delegate's filter/sort.
- **codon-panes** — zero tests. The adapter-registration pure logic
  (matching pane kinds to factory closures) is testable.
- **codon-session/src/actions.rs** — the registry has tests
  (`break_pane.rs`, `overview.rs`) but `actions.rs` does not.
  Add a capture/apply round-trip plus a focus-restoration test.
- **codon-keymap/src/cheatsheet_modal.rs** — modal rendering is
  hard to test, but the pure tab-filter / mode-filter logic isn't.
  Test that.

## Approach

For each crate / module:

1. Identify the pure functions (no `&mut App`, no `Context`, no I/O).
2. Add a `#[cfg(test)] mod tests` at the bottom of the relevant
   module file.
3. Write at least one test per pure function. Use the same shape
   the existing file-manager / codon-jump tests use — no
   `gpui::TestAppContext` if the function doesn't need it.

The bar is genuinely low: "at least one test per non-trivial pure
function". Sort/filter/state-transition logic is the canonical
target. Rendering is not.

## Test scaffolding

If a module needs an `App`-context for setup, the existing helper
is `gpui::TestAppContext::new`. For codon-session integration
tests, look at `crates/codon-session/src/break_pane.rs` for the
established pattern.

## Non-goals

- Not adding integration tests that exercise multiple crates.
  Unit tests only in this TASK.
- Not adding rendering / screenshot tests.
- Not adding tests for vendored Zed code.

## Files touched

- `crates/codon-agent/src/lib.rs` (or wherever `Explain` / `Summarize`
  / `Refactor` live) — add `#[cfg(test)] mod tests`.
- `crates/codon-pickers/src/dir_picker.rs` — add filter/sort tests.
- `crates/codon-panes/src/lib.rs` — add adapter registration test.
- `crates/codon-session/src/actions.rs` — add capture/apply tests.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — add tab/filter
  tests.

## Verification

- `cargo test -p codon-agent -p codon-pickers -p codon-panes -p codon-session -p codon-keymap`
  — passes. At least one new test per listed crate/module.
- ```
  for crate in codon-agent codon-pickers codon-panes codon-mode; do
    rg -l '#\[cfg(test)\]' crates/$crate/src/ || echo "still missing: $crate"
  done
  ```
  No "still missing" output.
