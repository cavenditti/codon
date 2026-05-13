---
id: TASK:phase-6/fm-gitignore-toggle
type: task
status: accepted
version: 0.0.1
summary: >
  `zg` hides / shows git-ignored entries — orthogonal to the existing
  `.` hidden-files toggle.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-sort-display#c-gitignore-toggle
---

# File-manager gitignore toggle

## What ships

A `show_gitignored: bool` field on `FileManager` (default: `true`
— matching today's behavior of showing everything). `zg`
(chord: `z` then `g`) flips it. When false, `read_dir_sync`
filters out entries whose status is `Ignored` per the
`project.git_store()` lookup the git-decorations clause already
consults.

Independent of `.` (hidden-files): a `.gitignore`'d hidden file
shows only when both toggles allow it.

## Where it slots in

- `show_gitignored` field on FM state, persisted via codon-config.
- Reuse the per-entry git-status fetch already in place for
  decorations.
- ~70 LOC.
