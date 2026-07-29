---
id: TASK:phase-24/fm-preview-pending-prefetch
type: task
status: accepted
version: 0.1.0
summary: >
  Make preview source/freshness visible and add capped adjacent-entry
  prefetch.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-stateful-ux#c-preview-pending-prefetch
blocked_by:
  - TASK:phase-24/fm-adaptive-enrichment
---

# Pending preview and bounded prefetch

Track displayed and requested preview paths separately, render a source
label/loading treatment when they differ, and prefetch the nearest
entries into a byte-bounded LRU after explicit work is idle.

## Acceptance

- Old preview data is never presented as belonging to the new cursor.
- Prefetch covers at most the configured adjacent window and byte cap.
- Entering a prefetched directory promotes its listing without I/O.
