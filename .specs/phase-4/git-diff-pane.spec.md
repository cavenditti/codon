---
id: TASK:phase-4/git-diff-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Refactor project_diff into a codon pane that renders through
  &dyn codon_buffer::Buffer and is openable standalone.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/git-pane#c-diff
---

# Git diff as a codon pane

## What ships

A pane that shows file-by-file diffs (working tree vs HEAD by default,
arbitrary refs configurable). Side-by-side or unified rendering. Each
hunk is a navigable unit (j/k between hunks). Selecting a hunk
exposes it as `Selection::Hunks(Vec<HunkRef>)` for cross-pane verbs.

## Where it comes from

- [`vendor/zed/crates/git_ui/src/project_diff.rs`](spec:src:vendor/zed/crates/git_ui/src/project_diff.rs)
  (~97 KB) is already a workspace Item — it just needs to be reshaped
  to take `&dyn codon_buffer::Buffer` at its inputs and live in the
  codon-git crate.
- `buffer_diff::DiffHunk` + `DiffHunkStatus` already provide every
  domain type we need
  ([`vendor/zed/crates/buffer_diff/src/buffer_diff.rs`](spec:src:vendor/zed/crates/buffer_diff/src/buffer_diff.rs)).

## Approach

Move project_diff out of `git_ui` (or wrap it) so the pane lives in
`crates/codon-git/`. Wire the buffer inputs through the `Buffer`
trait. Add a `Selection::Hunks` source impl so the agent's
`AgentExplain`-style verbs can target hunks too.

Default keymap: `cmd-k g d`. Phase 5's standalone diff-viewer pane
will be a thin wrapper around the same component.
