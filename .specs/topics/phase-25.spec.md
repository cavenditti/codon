---
id: TOPIC:topics/phase-25
type: topic
status: draft
version: 0.1.0
summary: >
  FM responsiveness follow-ups: the seven gaps left open by the phase-24
  render/IO/stateful-UX specs — in-memory re-sort, path-keyed marks,
  operation progress + cancel, measured paging, async config writes,
  preview I/O budget, and the async search refactor.
owners: [carlo]
---

# Phase 25

Phase 25 closes the file-manager gaps identified in the 2026-07-29 FM
performance/UX review that the phase-24 drafts
(`REQ:codon/fm-render-production`, `REQ:codon/fm-io-scheduler`,
`REQ:codon/fm-stateful-ux`) deliberately do not cover.

Three requirement areas, one task per clause under `.specs/phase-25/`:

- `REQ:codon/fm-listing-model` — sort changes re-sort in memory instead
  of re-reading the disk (`#c-in-memory-resort`), and marks are keyed by
  path (`#c-path-keyed-marks`). Complements phase-24's
  `REQ:codon/fm-stateful-ux#c-selection-path-continuity`, which covers
  the cursor; the multi-select marked set is specified here.
- `REQ:codon/fm-op-responsiveness` — progress + cancel for every
  mutating operation (complements phase-24's
  `REQ:codon/fm-stateful-ux#c-operation-undo`, which is
  post-completion), page motions derived from the measured viewport,
  debounced off-thread preference/bookmark writes, and bounded preview
  I/O.
- `REQ:codon/fm-search-async` — the standalone search refactor:
  streamed results, cancellation with child-process cleanup,
  incremental refiltering, and off-thread availability probes.
