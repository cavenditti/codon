---
id: TASK:phase-24/fm-directory-watch
type: task
status: accepted
version: 0.1.0
summary: >
  Watch active/parent directories and apply debounced listing deltas
  with targeted cache invalidation.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-io-scheduler#c-directory-watch
blocked_by:
  - TASK:phase-24/fm-render-cache-correctness
---

# Live directory updates

Use the existing `fs::Fs` watch surface for the current and parent
directories. Coalesce event bursts, reread only affected directories
when event detail is insufficient, and rotate watches on navigation.

## Acceptance

- External create/delete/rename appears without manual reload.
- Selection and scroll remain stable for unrelated changes.
- Dropped panes/tasks release their watchers.
