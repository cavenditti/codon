---
id: TASK:phase-4/git-hunk-staging
type: task
status: accepted
version: 0.0.2
summary: >
  Keyboard hunk staging from the diff pane — deferred. The
  underlying actions exist upstream; codon-side binding work waits
  on git-diff-pane.
owners: [carlo]
progress: deferred
refines:
  - REQ:codon/git-pane#c-hunk-staging
---

# Hunk staging from the diff pane

## Deferred (2026-05-12)

The actions needed already exist upstream: `git::StageAndNext`,
`git::UnstageAndNext`, `git::ToggleStaged`
([`vendor/zed/crates/git/src/git.rs:37`](spec:src:vendor/zed/crates/git/src/git.rs)).
What remains is the codon-side keymap binding — `s` / `u` / `S` / `U`
under a `GitDiff && pane_mode == normal` predicate, similar to the
GitPanel pattern that just landed.

That binding work is paired with
[TASK:phase-4/git-diff-pane](spec:TASK:phase-4/git-diff-pane), which
is also deferred. The pre-existing Zed `ProjectDiff` pane is already
"good enough" for day-to-day diff browsing — staging from inside it
works via the existing keymap, just not under codon-modal predicates
yet.

When git-diff-pane is revisited (codon-modal-integrate `ProjectDiff`
the same way we did `GitPanel`), the hunk-staging keys ride along in
the same `[bindings.git_diff.normal]` block. Until then, no
standalone work.
