---
id: TASK:phase-4/git-blame-verb
type: task
status: accepted
version: 0.0.1
summary: >
  Cross-pane git::BlameShow action that consumes Selection::Hunks
  and renders blame metadata for each hunk's line range.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/git-pane#c-blame-verb
---

# git.blame.show cross-pane verb

## What ships

A new action `codon_git::BlameShow`, registered with
`ActionAcceptsRegistry` for `ObjectKind::Hunk`. Behaviour:

1. Read `Selection::Hunks(Vec<HunkRef>)` from the focused pane (the
   diff pane is the primary source; the editor can also expose this
   for the current cursor line).
2. For each hunk, query `git2::blame_file` over `line_start..line_end`.
3. Render the blame metadata (commit sha, author, date, summary) in
   a popover or a short-lived blame pane below the diff.

## Where it comes from

- `Selection::Hunks` and `HunkRef` already exist in
  [`crates/codon-mode/src/selection.rs`](spec:src:crates/codon-mode/src/selection.rs).
- [`vendor/zed/crates/git_ui/src/blame_ui.rs`](spec:src:vendor/zed/crates/git_ui/src/blame_ui.rs)
  has the inline-gutter blame renderer — reuse the metadata-fetch path,
  swap the renderer for a popover.

## Approach

Cleanest as a new action in `codon-git` (or a sibling `codon-git-verbs`
module). Default keymap: `cmd-k g b`. Cheap to ship once the diff
pane exposes its `Selection::Hunks`.
