---
id: TASK:phase-2/persistence-shutdown-flush
type: task
status: accepted
version: 0.1.0
summary: >
  on_app_quit callback writes one final snapshot before exit.
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-shutdown-flush
---

# Shutdown flush

Registered in `codon_session::registry::init`.
