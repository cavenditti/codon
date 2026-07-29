---
id: REQ:codon/fm-listing-model
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Sort changes and marks operate on the in-memory listing model:
  re-sorting never re-reads the disk, and marks are path-keyed so they
  survive reload, re-sort, filtering, and watcher deltas.
owners: [carlo]
refines: []
categorized_under: []
---

# In-memory listing model

:::{requirement id="fm-listing-model" level="MUST"}
The system MUST:

- {#c-in-memory-resort} re-sort the already-loaded listing in memory
  when the sort mode or direction changes. Sort options MUST NOT
  participate in the directory-cache key, a sort change MUST NOT
  trigger a directory re-read or re-stat, the comparator MUST NOT
  allocate per comparison (case-fold and extension keys are precomputed
  per entry), and a listing read MUST stat each entry at most once.
- {#c-path-keyed-marks} key marks by canonical path rather than row
  index. Marks MUST survive reload, re-sort, filter apply/clear, and
  watcher deltas; marks whose paths disappear from the listing are
  dropped. Mark highlighting MUST never bleed across columns via index
  collisions, and the marked count/size footer MUST reflect the
  surviving set.
:::
