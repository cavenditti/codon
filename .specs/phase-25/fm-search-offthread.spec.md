---
id: TASK:phase-25/fm-search-offthread
type: task
status: accepted
version: 0.1.0
summary: >
  Move binary-availability probes and zoxide queries off the UI
  thread; pickers open instantly in a loading state.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-search-async#c-offthread-probes"]
assignee:
eta:
blocked_by: []
---

# Fm search offthread

## Plan

Refines `REQ:codon/fm-search-async#c-offthread-probes`.

[binary_available → which](spec:src:crates/file-manager/src/search.rs:409-418)
stats every `$PATH` entry synchronously and is called on the UI thread
when opening content search
([file_manager.rs:2500](spec:src:crates/file-manager/src/file_manager.rs:2500))
and the zoxide picker
([file_manager.rs:2514](spec:src:crates/file-manager/src/file_manager.rs:2514));
[zoxide_query](spec:src:crates/file-manager/src/search.rs:1136-1153)
spawns and waits on a child process synchronously at
[file_manager.rs:2521](spec:src:crates/file-manager/src/file_manager.rs:2521).

- Run the probe and the zoxide query on the background executor; open
  the modal immediately in a loading state and fill (or surface the
  "binary missing" hint) when the result lands.
- Cache `which` results for the process lifetime — availability does
  not change keystroke-to-keystroke.

## Acceptance

- Opening the content-search and zoxide pickers performs no
  synchronous `$PATH` stat or child spawn on the foreground thread
  (code-path assertion or executor-tag test).
- Cold-cache open shows the modal with a loading row instantly; the
  missing-binary hint still appears when `rg`/`zoxide` is absent.
- Probe results are cached; reopening a picker does not re-stat
  `$PATH`.
