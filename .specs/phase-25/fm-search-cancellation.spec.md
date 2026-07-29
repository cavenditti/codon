---
id: TASK:phase-25/fm-search-cancellation
type: task
status: accepted
version: 0.1.0
summary: >
  Cancel in-flight fuzzy matching and kill spawned fd/rg children when
  a query is superseded or the search modal is dismissed.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-search-async#c-cancellation-cleanup"]
assignee:
eta:
blocked_by: []
---

# Fm search cancellation

## Plan

Refines `REQ:codon/fm-search-async#c-cancellation-cleanup`.

The `fuzzy::match_strings` cancel flag is constructed `false` and
never set — name search at
[search.rs:137](spec:src:crates/file-manager/src/search.rs:137) /
[145](spec:src:crates/file-manager/src/search.rs:145), content search
at [564](spec:src:crates/file-manager/src/search.rs:564) /
[572](spec:src:crates/file-manager/src/search.rs:572) — so rapid
typing races N concurrent match passes, last-writer-wins. Dismissing
the modal drops the producer task
([search.rs:205](spec:src:crates/file-manager/src/search.rs:205),
[238](spec:src:crates/file-manager/src/search.rs:238)) but the
already-running `fd`/`rg` child continues to completion — there is no
`Child::kill` anywhere in the crate.

- Hold the `Child` handle; on modal dismiss or query supersede, kill
  and reap it.
- Set the shared `AtomicBool` for superseded fuzzy passes; tie both to
  a query generation so a stale pass can never publish.
- Coordinate the generation scheme with
  `TASK:phase-25/fm-search-refilter`.

## Acceptance

- Dismissing the modal mid-search leaves no running `fd`/`rg` process
  (test polls the child until exit).
- Superseding a query flips the prior pass's cancel flag (unit test at
  the delegate level).
- Double-dismiss and dismiss-during-spawn race cleanly (no panic, no
  leaked child).
