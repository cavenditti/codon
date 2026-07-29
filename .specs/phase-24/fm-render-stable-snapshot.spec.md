---
id: TASK:phase-24/fm-render-stable-snapshot
type: task
status: accepted
version: 0.1.0
summary: >
  Replace per-render listing clones and derived O(N) scans with stable
  immutable render snapshots updated only when model state changes.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-render-production#c-stable-snapshots
blocked_by:
  - TASK:phase-24/fm-trace-completed-frame
---

# Stable FM render snapshots

Store current, parent, and directory-preview entries in stable
`Arc`-backed snapshots usable by both renderers. Persist the shaped-line
caches on the FM entity and cache listing/mark aggregates.

## Acceptance

- Repaint allocates/clones only the visible range and small state
  bundles.
- A 500-entry and 5,000-entry listing have comparable warm repaint
  distributions.
- Listing, mark, filter, and enrichment mutations rebuild only their
  affected snapshot/aggregate.
