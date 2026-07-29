---
id: TASK:phase-24/fm-selection-path-continuity
type: task
status: accepted
version: 0.1.0
summary: >
  Preserve file-manager selection by path across model reorder and live
  updates.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-stateful-ux#c-selection-path-continuity
blocked_by:
  - TASK:phase-24/fm-directory-watch
---

# Path-anchored selection

Capture the selected path before sort/filter/reload/delta application,
restore its new index afterward, and choose the nearest surviving
neighbor only when that path disappears.

## Acceptance

- Sort, git filtering, child-count fill, and unrelated watcher changes
  retain the same selected path.
- Deleting the selected entry chooses a deterministic neighbor and
  requests exactly one new preview.
