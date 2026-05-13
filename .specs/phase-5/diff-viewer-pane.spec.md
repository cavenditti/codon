---
id: TASK:phase-5/diff-viewer-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Standalone diff viewer pane — thin wrapper over the Phase 4 git diff
  pane, openable on arbitrary inputs (file vs file, file vs HEAD,
  buffer vs disk).
owners: [carlo]
progress: done
refines:
  - REQ:codon/additional-panes#c-diff-viewer
---

# Standalone diff viewer pane

## What ships

A `workspace::Item` pane that renders a two-buffer diff. Opens via:

- `cmd-k d d` from the file manager when two entries are marked
- An action `codon::DiffOpen(left, right)` dispatchable from anywhere

## Why it's a wrapper

The actual diff rendering is owned by
[TASK:phase-4/git-diff-pane](spec:TASK:phase-4/git-diff-pane) — that
work produces a reusable component over Zed's concrete
`language::Buffer`. The diff viewer just constructs that component
from arbitrary buffers (not just working tree vs HEAD).

## Approach

Block on Phase 4's git diff pane landing. After that, this pane is
~100 LOC of glue: a `DiffViewer { left, right, inner: DiffComponent }`
plus the action registration. Most of the effort is in choosing how
the two buffers are sourced (file manager marks, command palette
input, etc.).
