---
id: TASK:phase-2/codon-session-crate
type: task
status: accepted
version: 0.1.0
summary: >
  New crate codon-session with Session/Window/SessionId/WindowId types and a SessionRegistry global.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-data-model
---

# codon-session crate scaffold

Created in commit 85e0faa. Lives at `crates/codon-session/`. Implements `Session`, `Window`, `SessionId`, `WindowId`, and `SessionRegistry` (Arc-cloneable, KVP-backed).
