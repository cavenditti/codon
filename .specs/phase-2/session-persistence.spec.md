---
id: TASK:phase-2/session-persistence
type: task
status: accepted
version: 0.1.0
summary: >
  JSON-encoded SessionRegistry persisted under KVP key codon_sessions_v1.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-persistence
---

# Session KVP persistence

Read on init, written by every action handler via `persist_async`. Backed by `db::kvp::GlobalKeyValueStore`.
