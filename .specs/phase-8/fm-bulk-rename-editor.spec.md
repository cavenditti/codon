---
id: TASK:phase-8/fm-bulk-rename-editor
type: task
status: accepted
version: 0.0.1
summary: >
  `cw` opens a workspace buffer with marked entries' names, one per
  line. Save + close applies the line-for-line rename atomically.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-bulk-editor#c-bulk-rename-editor
---

# File-manager bulk rename via $EDITOR

## What ships

`cw` (change-word) on a marked set:

1. Write the marked entries' current names — one per line, in
   FM display order — into a workspace buffer titled `Bulk
   rename — N files`. Buffer is unsaved-on-disk; just a Zed
   buffer the user edits in place.
2. On buffer save AND close, codon diffs original→edited
   line-by-line.
3. Line-count change → toast "Bulk rename expects N lines, got
   M; nothing applied"; revert and exit.
4. Apply renames in order via `fs::Fs::rename`. Collect into a
   single notification (phase-8/fm-task-as-notifications).
   First-failure rolls back already-applied renames.

Complements (does not replace) the phase-5 `R` pattern-based
rename — that one stays for structured / numbered series.

## Where it slots in

- New buffer-bridge module under
  [`crates/file-manager/src/`](spec:src:crates/file-manager/src/) —
  creates a transient `language::Buffer`, subscribes to its save
  + close events.
- ~250 LOC including the rollback path.
