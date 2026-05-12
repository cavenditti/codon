---
id: TASK:phase-5/file-manager-tests
type: task
status: accepted
version: 0.0.1
summary: >
  Add unit tests for the file manager's pure logic — directory
  sorting, hidden filtering, navigation bounds, mark-set
  transitions, pending-input state — so future refactors (fuzzy
  filter, git indicators, bulk ops) have a safety net.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-test-coverage-floor
---

# Unit tests for file-manager pure logic

## What ships

`crates/file-manager/` ships with zero unit tests today. Most of the
crate is GPUI rendering, which is rightly untested at the unit level
— but several pure functions are testable in isolation:

- `read_dir_sync` (around `file_manager.rs:1110`) — sort order,
  hidden filtering, the dotfile decision, error recovery on
  unreadable subdirectories.
- Navigation helpers — `nav_down`, `nav_up`, `nav_page_down`,
  `nav_page_up`, `nav_top`, `nav_bottom` — bounds and wrap behaviour.
- Mark-set transitions — `mark_toggle`, `mark_clear` — idempotency,
  preservation of mark on entry mutation, behaviour when the
  marked entry no longer exists after a refresh.
- Pending-input state — set, commit (once TASK:phase-5/file-manager-handler-commit
  lands), cancel.

GPUI rendering, focus events, and the trait impls do **not** need
coverage at this stage.

## What changes

- A new `crates/file-manager/src/tests/` (or inline `#[cfg(test)] mod
  tests` per file once the decomposition in
  TASK:phase-5/file-manager-decompose lands).
- A small fixtures helper that builds a tempdir tree with known
  entries (visible, hidden, subdirs, symlinks if cheap).
- The async / Fs-trait paths can be tested with the `fake_fs`
  helpers in `vendor/zed/crates/fs/` — use them rather than touching
  real disk where possible.

## Sequencing

Best landed *after* TASK:phase-5/file-manager-decompose so the tests
sit next to the modules they cover. If decomposition slips, this
task lands as a single `tests.rs` next to `file_manager.rs`.

## File anchors

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
- [`vendor/zed/crates/fs/src/fake_fs.rs`](spec:src:vendor/zed/crates/fs/src/fake_fs.rs)
  — the existing fake filesystem helpers.

## Acceptance

- `cargo test -p file-manager` runs at least one test per function
  category listed above.
- Every test uses `fake_fs` or a tempdir — no test depends on the
  developer's `$HOME` or working directory.
- Hidden-filter test exercises both the `.` toggle and the
  show_hidden=true / false branches.
- Navigation test covers single-entry directory, empty directory,
  and a directory of 1000 entries (bounds + Ctrl-d/u math).

## Out of scope

- Integration tests that drive the full GPUI render loop. Possible
  in principle (Zed's test harness supports it), but a separate
  task once we have a pattern for it.
- Snapshot tests for column layout / styling.

Effort: small-to-medium. ~250 LOC of tests, depending on how
generously `fake_fs` covers our needs.
