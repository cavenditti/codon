---
id: TASK:phase-2/stack-fallback
type: task
status: accepted
version: 0.1.0
summary: >
  Applying a LayoutSnapshot::Stack falls back to its active member.
owners: [carlo]
progress: done
refines:
  - REQ:codon/layout#c-stack-fallback
---

# Stack snapshot fallback

Implemented in `LayoutSnapshot::into_serialized` so existing Member variants suffice.
