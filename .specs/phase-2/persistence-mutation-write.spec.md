---
id: TASK:phase-2/persistence-mutation-write
type: task
status: accepted
version: 0.1.0
summary: >
  Per-mutation persist via persist_async after every session/window action.
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-mutation-write
---

# Per-mutation KVP write

Each action handler calls `persist_async(cx)` after mutating the registry.
