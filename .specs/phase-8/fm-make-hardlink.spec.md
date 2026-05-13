---
id: TASK:phase-8/fm-make-hardlink
type: task
status: accepted
version: 0.0.1
summary: >
  `Ln` chord — create hardlinks. Toast when source and target are on
  different filesystems (hardlinks fail across mounts).
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-symlinks#c-make-hardlink
---

# File-manager create hardlink

## What ships

`L`-then-`n` (shift-l then n) chord. Same flow as
TASK:phase-8/fm-make-symlink but calls `std::fs::hard_link`
(via the `Fs` trait's new `create_hardlink` method).

Pre-check: detect different-filesystem (same dev/inode root) and
surface a toast before attempting, since the OS error is cryptic.
Fallback to the libc `statfs` / `metadata.dev()` check.

## Where it slots in

Sibling to TASK:phase-8/fm-make-symlink. ~50 LOC including the
cross-fs pre-check.
