---
id: TASK:phase-2/persistence-heartbeat
type: task
status: accepted
version: 0.1.0
summary: >
  30-second background heartbeat re-persists the registry.
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-heartbeat
---

# 30s persistence heartbeat

Implemented as `spawn_heartbeat(cx)` in `codon_session::registry`.
