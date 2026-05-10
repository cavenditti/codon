---
id: TASK:phase-2/session-close
type: task
status: accepted
version: 0.1.0
summary: >
  SessionClose action removes the active session, refusing to remove the last.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-close
---

# Session close action

Implemented in `codon_session::actions::handle_session_close`. Returns SessionRegistryError::LastSession when only one session exists, otherwise removes and selects the next.
