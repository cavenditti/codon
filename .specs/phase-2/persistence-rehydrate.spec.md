---
id: TASK:phase-2/persistence-rehydrate
type: task
status: accepted
version: 0.1.0
summary: >
  Registry loads from KVP at codon_session::init.
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-rehydrate
---

# Startup rehydrate

Reads `codon_sessions_v1` from the global KVP, deserializes JSON. Per-window layouts restore via SerializedPaneGroup::deserialize.
