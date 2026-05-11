---
id: TASK:phase-4/git-log-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Standalone git log pane with branch / graph rendering, j/k nav,
  Enter to open the commit's diff.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/git-pane#c-log
---

# Git log as a codon pane

## What ships

A pane listing commits in reverse-chronological order, with ASCII
branch / merge graph on the left and `<sha> <subject> (<author>)` on
the right. j/k navigates, Enter opens the commit's diff in the diff
pane.

## Where it comes from

- The `git_graph` crate (already vendored at
  [`vendor/zed/crates/git_graph/`](spec:src:vendor/zed/crates/git_graph/))
  produces the graph layout.
- Commit metadata via `git::repository::log` or the underlying
  `git2::Revwalk`.
- Today log lives inside
  [`vendor/zed/crates/git_ui/src/commit_view.rs`](spec:src:vendor/zed/crates/git_ui/src/commit_view.rs)
  (~41 KB). Extract the log-rendering bits into a standalone pane.

## Approach

New module under `crates/codon-git/` (or the same crate
git-status-pane lives in). Implements `workspace::Item`, holds a
`Vec<CommitRow>` populated by an async git log query (background task,
update via `cx.emit` so the pane re-renders).

Default keymap: `cmd-k g l`. Cross-pane verb consumed by
`codon-agent` (an agent action can take a `Selection::Commits(Vec<String>)`
sourced from this pane).
