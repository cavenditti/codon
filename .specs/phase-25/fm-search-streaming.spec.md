---
id: TASK:phase-25/fm-search-streaming
type: task
status: accepted
version: 0.1.0
summary: >
  Stream fd/rg stdout into the search pickers incrementally and
  surface candidate-cap truncation instead of discarding the flag.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-search-async#c-streaming-results"]
assignee:
eta:
blocked_by: []
---

# Fm search streaming

## Plan

Refines `REQ:codon/fm-search-async#c-streaming-results`.

All three producers block on `Command::output()` — fd at
[search.rs:321](spec:src:crates/file-manager/src/search.rs:321), rg at
[search.rs:794](spec:src:crates/file-manager/src/search.rs:794),
zoxide at
[search.rs:1137](spec:src:crates/file-manager/src/search.rs:1137) —
so the 64-/32-row "batches"
([355-358](spec:src:crates/file-manager/src/search.rs:355-358),
[823-826](spec:src:crates/file-manager/src/search.rs:823-826)) are
carved out of an already-complete buffer, and nothing renders until
the child exits. The walkdir truncation flag is computed then
discarded at
[search.rs:301](spec:src:crates/file-manager/src/search.rs:301)
despite the doc at
[366-368](spec:src:crates/file-manager/src/search.rs:366-368) saying
callers can surface a toast.

- Switch fd/rg to piped stdout consumed line-by-line on the background
  executor, appending genuine batches as they arrive; keep
  [MAX_CANDIDATES](spec:src:crates/file-manager/src/search.rs:25).
- Surface truncation (picker footer line or toast) for both the tool
  and walkdir paths.
- Correct the overselling doc comment at
  [787-789](spec:src:crates/file-manager/src/search.rs:787-789)
  ("parse the *streamed* output") to match the new, actually-streaming
  behavior.

## Acceptance

- With a slow producer (test fake or throttled fixture), first
  results are visible in the picker while the child is still running.
- Hitting the candidate cap shows a visible "capped at N" indicator in
  both the fd and walkdir paths.
- Doc comments describe the real streaming behavior.
