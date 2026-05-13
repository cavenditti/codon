---
id: TASK:phase-6/fm-archive-preview
type: task
status: accepted
version: 0.0.1
summary: >
  List archive contents in the preview column (top-level entries
  only, no extraction).
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-preview-richer#c-archive-preview
---

# File-manager archive preview

## What ships

For files whose extension is in
`["zip", "tar", "gz", "tgz", "tar.gz", "7z"]`, the preview column
lists the archive's top-level entries (filename + size). Long
archives truncate at 200 entries with a `… N more` line.

No extraction — just open-and-list. The crates are read-only here.

## Approach

1. Extend `Preview` enum with `Archive { entries: Vec<ArchiveEntry> }`.
2. Per-format handler:
   - `.zip` → `zip` crate's `ZipArchive::file_names()`.
   - `.tar` / `.tar.gz` / `.tgz` → `tar` crate (`gzip` chained
     when `.gz`).
   - `.7z` → `sevenz-rust` if not already pulled in transitively;
     gate on it being available — otherwise fall through to the
     binary fallback.
3. Render as a `v_flex` of muted `Label`s, mirroring the dir
   children preview already in place.

~150 LOC; ~50 LOC of dependency wiring depending on which crates
are already in the workspace graph.
