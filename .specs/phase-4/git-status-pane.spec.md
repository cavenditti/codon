---
id: TASK:phase-4/git-status-pane
type: task
status: accepted
version: 0.0.2
summary: >
  Originally: ship a new codon pane (workspace::Item) duplicating the
  status tree. Pivoted to modal-integrate the existing Zed GitPanel
  dock instead — see TASK:phase-4/git-panel-modal-integration.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/git-pane#c-status
---

# Git status as a codon pane

## Outcome: wontdo

We *shipped* this as `crates/codon-git` (commit `a48a81c`,
`workspace::Item` with three sections + j/k/s/u/Enter), used it, and
realised the new pane duplicates ~85% of what Zed's GitPanel dock
already does (staged / unstaged / untracked sections, the commit
editor, conflict view, AI commit messages — ~6000 lines of working
code). Reimplementing from scratch was wasted work.

The pivot — keep using GitPanel and **fit codon's modal model around
it** — is tracked under
[TASK:phase-4/git-panel-modal-integration](spec:TASK:phase-4/git-panel-modal-integration).
The clause `REQ:codon/git-pane#c-status` remains satisfied: by the
dock, not by a fresh pane.

`crates/codon-git` and the `cmd-k g s = codon_git::OpenStatusPane`
keymap line are removed. `cmd-k g s` now binds
`git_panel::ToggleFocus` so the same muscle memory opens the dock.

## Why the original approach didn't land

- Two surfaces for the same data is a maintenance tax — every diff
  against upstream Zed's `GitPanel` would need a parallel
  consideration of whether our pane needs the same change.
- The dock's keyboard surface was *almost* usable already; what it
  was missing was codon-style modal predicates (no `pane_mode`),
  no `:` palette wiring, and no `j`/`k`/`Enter`/`s`/`u`
  defaults. All three are cheaper to add than to re-implement the
  underlying view.
- "Everything is a pane" is a codon principle, but for git
  specifically the dock placement (resizable side rail with the
  commit editor) is better than a flat pane — committing is a
  separate sub-task from browsing changes.
