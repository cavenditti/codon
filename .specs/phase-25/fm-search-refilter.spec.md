---
id: TASK:phase-25/fm-search-refilter
type: task
status: accepted
version: 0.1.0
summary: >
  Refilter incrementally as batches arrive instead of rebuilding the
  full candidate set on every batch.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-search-async#c-incremental-refilter"]
assignee:
eta:
blocked_by: []
---

# Fm search refilter

## Plan

Refines `REQ:codon/fm-search-async#c-incremental-refilter`.

`append_batch` calls `update_matches` after every batch
([search.rs:91](spec:src:crates/file-manager/src/search.rs:91),
[497](spec:src:crates/file-manager/src/search.rs:497)), and
`update_matches` rebuilds the entire `StringMatchCandidate` vec from
scratch each time
([130-135](spec:src:crates/file-manager/src/search.rs:130-135),
[557-562](spec:src:crates/file-manager/src/search.rs:557-562)). At
the 5000-candidate cap in 64-row batches that is ~79 rebuilds
averaging ~2500 entries — roughly 200k candidate constructions plus
79 spawned fuzzy passes for a single search. The content variant also
rebuilds `self.matches` inline before calling `update_matches`
([481-491](spec:src:crates/file-manager/src/search.rs:481-491)).

- Keep a persistent, append-only candidate vec; on batch arrival run
  the fuzzy pass over the new tail and merge into the ranked result
  set (or debounce full passes behind the query generation from
  `TASK:phase-25/fm-search-cancellation`).
- Remove the duplicate inline rebuild in the content delegate.

## Acceptance

- Total candidate constructions across a full capped stream are O(n),
  verified with a counter test (today's behavior is O(n²) per batch
  count).
- Typing while results are still streaming stays responsive: no full
  rebuild per (keystroke × batch) pair.
- Ranked results for a settled query are identical before/after the
  refactor (golden test).
