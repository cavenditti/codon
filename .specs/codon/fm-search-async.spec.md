---
id: REQ:codon/fm-search-async
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  File-manager search pickers stream child output, cancel cleanly
  (including spawned child processes), refilter incrementally instead
  of rebuilding per batch, and never run probe I/O on the UI thread.
owners: [carlo]
refines: []
categorized_under: []
---

# Async file-manager search

:::{requirement id="fm-search-async" level="MUST"}
The system MUST:

- {#c-streaming-results} consume search-tool output incrementally so
  the first results render before the child process exits, and surface
  truncation at the candidate cap to the user instead of silently
  discarding the flag.
- {#c-cancellation-cleanup} cancel in-flight fuzzy matching when a
  query is superseded or the modal is dismissed, and terminate spawned
  search child processes on dismissal — no orphaned `fd`/`rg` after
  the modal closes.
- {#c-incremental-refilter} refilter arriving batches without
  rebuilding the full candidate set per batch; superseded refilter
  passes MUST be cancelled or generation-checked so a stale pass can
  never overwrite a newer result set.
- {#c-offthread-probes} run binary-availability probes and zoxide
  queries off the UI thread; opening a picker MUST NOT stat `$PATH`
  entries or spawn helper processes synchronously.
:::
