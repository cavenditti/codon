---
id: TASK:phase-4/git-hunk-staging
type: task
status: accepted
version: 0.0.1
summary: >
  Keyboard hunk staging from the diff pane — s / u stage/unstage the
  current hunk, S / U do the same for the active file.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/git-pane#c-hunk-staging
---

# Hunk staging from the diff pane

## What ships

Inside the git diff pane:

- `s` — stage the hunk under the cursor
- `u` — unstage the hunk under the cursor
- `S` — stage the entire file
- `U` — unstage the entire file

Hunks display their staged-ness via `DiffHunkSecondaryStatus` (the
field is already on `DiffHunk`, just unread by the current UI).

## Where it comes from

- `git::repository` exposes `stage_paths` / `unstage_paths`
  — same calls git_panel already uses.
- `DiffHunkSecondaryStatus`:
  [`vendor/zed/crates/buffer_diff/src/buffer_diff.rs`](spec:src:vendor/zed/crates/buffer_diff/src/buffer_diff.rs)

## Approach

Action handlers on the diff pane (`codon_git::StageHunk`,
`UnstageHunk`, `StageFile`, `UnstageFile`). Each computes the hunk's
byte range and calls the repository API with a partial-stage payload.
Re-fetch the diff after the stage call so the secondary status
re-renders. ~1 week of work.
