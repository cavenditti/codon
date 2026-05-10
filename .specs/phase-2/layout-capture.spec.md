---
id: TASK:phase-2/layout-capture
type: task
status: accepted
version: 0.1.0
summary: >
  capture_layout walks the live Member tree and returns a LayoutSnapshot.
owners: [carlo]
progress: done
refines:
  - REQ:codon/layout#c-capture
---

# Capture live layout

In `workspace::codon_bridge::capture_layout`. Walks recursively over Member::Axis / Member::Pane.
