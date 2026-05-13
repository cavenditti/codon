---
id: TASK:phase-4/git-diff-pane
type: task
status: accepted
version: 0.0.2
summary: >
  Git diff as a codon pane — deferred. Zed's pre-existing ProjectDiff
  pane already works fine for day-to-day use; modal-integration is a
  future polish item.
owners: [carlo]
progress: deferred
refines:
  - REQ:codon/git-pane#c-diff
---

# Git diff as a codon pane

## Deferred (2026-05-12)

Zed's `ProjectDiff` is already a `workspace::Item` with a stable
key context (`GitDiff`,
[`vendor/zed/crates/git_ui/src/project_diff.rs:1119`](spec:src:vendor/zed/crates/git_ui/src/project_diff.rs))
and side-by-side rendering of every changed file. Opened today via
`git::Diff` / `git::BranchDiff` actions; works without a codon
patch.

What's missing for full codon-modal integration:

- The dispatch context doesn't publish `pane_mode`, so codon's
  `[bindings.git_diff.normal]` predicates wouldn't match.
- `focus_in` doesn't write `CodonModeTracker`, so the status-bar
  mode pill doesn't follow.

Both fix to the same surgical patch we applied to `GitPanel`
(`vendor/zed/crates/git_ui/src/git_panel.rs`, commit `0a7cc8379b`).
But the pre-existing pane is *already usable* — Helix-mode editor
navigation moves between hunks, file switching works, `cmd-c`
copies. The codon-modal layer would buy us `:` palette + Helix-style
verbs (`j`/`k` / `s`/`u` / etc.), which is nice but not blocking.

Deferring to a future pass alongside
[TASK:phase-4/git-hunk-staging](spec:TASK:phase-4/git-hunk-staging),
which would naturally live in the same `[bindings.git_diff.normal]`
block. The original framing about "refactor through `&dyn
codon_buffer::Buffer`" is moot — `REQ:codon/buffer-trait` is
superseded (Helix-as-engine integration is wontdo) and
`ProjectDiff` works fine on Zed's concrete `Buffer`.
