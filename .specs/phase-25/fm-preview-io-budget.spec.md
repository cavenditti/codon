---
id: TASK:phase-25/fm-preview-io-budget
type: task
status: accepted
version: 0.1.0
summary: >
  Head-read the text-preview byte cap, name the editor-upgrade dwell
  constant, and give the upgraded-preview editor a small LRU.
owners: [carlo]
progress: pending
refines: ["REQ:codon/fm-op-responsiveness#c-preview-io-budget"]
assignee:
eta:
blocked_by: []
---

# Fm preview io budget

## Plan

Refines `REQ:codon/fm-op-responsiveness#c-preview-io-budget`.

- [read_text_preview](spec:src:crates/file-manager/src/file_manager.rs:5616-5619)
  reads the entire file into memory before checking
  [TEXT_PREVIEW_MAX_BYTES](spec:src:crates/file-manager/src/file_manager.rs:5610)
  — a multi-GB file is fully read then discarded. Switch to a bounded
  head read (`File` + `take(cap + 1)`), treating an over-cap read as
  the existing fall-through-to-binary case.
- The 150 ms editor-upgrade dwell is a bare literal inside
  [request_preview_update](spec:src:crates/file-manager/src/file_manager.rs:1249-1261)
  — hoist it to a named constant next to
  [PREVIEW_DEBOUNCE_MS](spec:src:crates/file-manager/src/file_manager.rs:219)
  (optionally exposed as an `[fm]` pref later).
- The upgraded-preview editor cache
  ([preview_editor](spec:src:crates/file-manager/src/file_manager.rs:340-343))
  holds exactly one path — alternating between two adjacent files
  rebuilds editor + buffer + language each way. Widen to a small LRU
  (2–4 entries).

## Acceptance

- Previewing a file larger than the cap reads at most cap + 1 bytes
  from disk (test via an instrumented reader or a fifo/sparse-file
  fixture).
- The dwell has exactly one named definition; no bare `150` remains in
  the preview path.
- Alternating `j`/`k` between two text files re-uses both cached
  editors (assertable via cache hit count / no rebuild).
