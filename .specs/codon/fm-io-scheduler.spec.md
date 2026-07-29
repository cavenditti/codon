---
id: REQ:codon/fm-io-scheduler
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Bound file-manager enrichment work, deduplicate repository status
  jobs, and react incrementally to filesystem changes without flooding
  the background executor.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-24]
---

# Bounded file-manager I/O

:::{requirement id="fm-io-scheduler" level="MUST"}
The system MUST:

- {#c-visible-child-counts} compute directory child counts only when
  the active line mode displays them and only for the visible window
  plus a small lookahead. Counts MUST be cached and filled with bounded
  concurrency.
- {#c-git-status-scheduler} coalesce and debounce git-status requests
  per repository, reuse a fresh result across focus and navigation,
  cancel or ignore superseded work, and avoid an unconditional
  whole-repository `--ignored` scan for every listing refresh.
- {#c-directory-watch} watch the active and parent directories and
  apply debounced insert/remove/rename/metadata deltas. Watch events
  MUST invalidate affected directory, preview, row-glyph, child-count,
  and git-status cache entries.
- {#c-adaptive-enrichment} paint names and basic type/icon data first,
  then schedule visible metadata, git state, and rich previews by
  priority. Background enrichment MUST never delay first listing paint.
:::
