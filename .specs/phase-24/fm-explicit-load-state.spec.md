---
id: TASK:phase-24/fm-explicit-load-state
type: task
status: accepted
version: 0.1.0
summary: >
  Represent listing loading and errors explicitly and prevent retained
  stale rows from receiving list-dependent operations.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-stateful-ux#c-explicit-load-state
blocked_by: []
---

# Explicit listing state

Change directory reads from `Vec<DirEntry>` to a result-bearing payload
and add Loading/Ready/Error state to `FileManager`. Retained rows may be
shown as stale context but cannot be mutated under the new path.

## Acceptance

- Empty, permission-denied, missing, and loading directories have
  distinct render states.
- Destructive and list-relative verbs reject stale rows with a concise
  message.
- Retry is keyboard-operable.
