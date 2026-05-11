---
id: TASK:phase-4/git-status-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Extract the status tree from git_panel.rs into a codon pane
  (workspace::Item), with staged / unstaged / untracked sections and
  keyboard navigation.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/git-pane#c-status
---

# Git status as a codon pane

## What ships

A new pane type that lists the working tree:

- staged changes
- unstaged changes
- untracked files

j/k navigates the list; Enter opens the file in an editor pane;
`s`/`u` stage/unstage the entry (calls `git::repository::stage` /
`unstage`).

## Where it comes from

- [`vendor/zed/crates/git_ui/src/git_panel.rs`](spec:src:vendor/zed/crates/git_ui/src/git_panel.rs)
  (~308 KB) — currently a `Panel` (sidebar dock). The status tree
  logic is the part we extract.
- `git::status::FileStatus` (`vendor/zed/crates/git/`) — domain type,
  reuse as-is.

## Approach

Either fork `git_ui` wholesale into `crates/codon-git/` or pull the
status-tree component out incrementally as a sub-module that implements
`workspace::Item`. Per the Phase 3 lesson (AgentPanel → Item refactor
was 2,400+ lines), prefer the **extract-into-new-crate** approach so
the diff is additive instead of touching every caller of
`panel::<GitPanel>()`.

Pane action: register a `codon_git::OpenStatusPane` action, default
keymap `cmd-k g s` (slotting next to the existing `cmd-k g m` →
`git::GenerateCommitMessage`).
