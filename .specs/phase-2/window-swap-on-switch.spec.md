---
id: TASK:phase-2/window-swap-on-switch
type: task
status: accepted
version: 0.1.0
summary: >
  Outgoing window's layout is captured before the incoming window's snapshot is applied.
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-swap-on-switch
---

# Capture-then-apply on window switch

Implemented in the `cycle_window` and `switch_to_window` paths. Uses `codon_session::swap::capture` and `apply`.
