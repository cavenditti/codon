---
id: TASK:phase-5/fm-git-indicators
type: task
status: accepted
version: 0.0.1
summary: >
  Per-entry git status decoration (M / A / D / ??) in the file
  manager, sourced from project.git_store().status_for_path().
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-enhancements#c-git-indicators
---

# Per-entry git status indicators

## What ships

Each file listing line shows a 1-char status badge:

- `M` modified
- `A` added (staged but new)
- `D` deleted
- `??` untracked
- (none) clean / ignored

## Where the data comes from

- [`vendor/zed/crates/project/src/git_store.rs`](spec:src:vendor/zed/crates/project/src/git_store.rs)
  exposes `FileStatus` per path (line 218 in the type).
- Reference renderer:
  [`vendor/zed/crates/project_panel/src/project_panel.rs`](spec:src:vendor/zed/crates/project_panel/src/project_panel.rs)
  (lines ~1067, ~6451–6479) shows the pattern.

## Approach

Add a `git_status: Option<GitStatus>` field to file-manager's
`DirEntry`. Populate it when entries reload by querying
`project.git_store().status_for_path(...)` for each entry. Render the
badge in `render_entry()` with a muted color column before the entry
name. ~40–60 LOC.
