---
id: TOPIC:topics/phase-24
type: topic
status: draft
version: 0.1.0
summary: >
  File-manager performance and stateful UX: promote the custom renderer
  to the production path behind trustworthy frame measurements, remove
  listing-sized work from repaint, bound enrichment I/O, react to
  external filesystem changes, and make loading, preview, selection,
  and undo state explicit to the user.
owners: [carlo]
---

# Phase 24 — File-manager performance and stateful UX

Phase 17 built the core custom row/column renderer, caches, async
preview pipeline, and render-trace surface. Phase 24 closes the gap
between that foundation and a production-quality file manager.

The phase has three workstreams:

1. **Production renderer.** Measure completed frames, keep render
   snapshots and caches stable across repaint, make cache invalidation
   correct, restore visual/interaction parity, and enable the fast path
   by default once it meets the acceptance gate.
2. **Bounded I/O.** Limit child-count and git enrichment to useful
   work, coalesce redundant tasks, and update listings incrementally
   from filesystem events.
3. **Stateful UX.** Represent loading and errors explicitly, avoid
   presenting stale preview data as current, preserve selection by
   path, prefetch conservatively, and make completed file operations
   undoable.

## Requirements

- [REQ:codon/fm-render-production](spec:REQ:codon/fm-render-production)
- [REQ:codon/fm-io-scheduler](spec:REQ:codon/fm-io-scheduler)
- [REQ:codon/fm-stateful-ux](spec:REQ:codon/fm-stateful-ux)

## Acceptance gates

- Completed-frame p95 is at most 5 ms and keypress-to-present p95 is at
  most 16 ms during the scripted 500-entry navigation replay.
- Repaint cost is proportional to visible rows, not listing length.
- No focus/navigation burst launches duplicate git-status jobs or
  child-count reads for offscreen directories.
- External directory mutations appear without manual reload while the
  cursor remains anchored to the same surviving path.
- Loading, permission errors, stale previews, and undo availability are
  visible and keyboard-operable.
