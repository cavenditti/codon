---
id: TASK:phase-9/fm-git-status-colors
type: task
status: accepted
version: 0.0.1
summary: >
  Stronger git-status tints — staged green-bold, modified yellow-bold,
  deleted red, untracked cyan, conflicted magenta — applied to both
  the leading status glyph and the filename text.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/file-manager-theme#c-git-status-colors
---

# File-manager git-status colors

## What ships

The current decoration helper in `file_manager.rs`
(`git_status_decoration`) returns a glyph + a single muted color.
Replace with a `git_status_palette` returning `(glyph, glyph_color,
filename_color)` keyed on `git::FileStatus`:

```
Index modified  ('M')  -> warning bold       (glyph + filename)
Worktree mod    ('M')  -> warning            (glyph + filename)
Index added     ('A')  -> created            (glyph + filename)
Worktree added  ('?')  -> info               (glyph muted, filename info)
Deleted         ('D')  -> deleted            (glyph + filename)
Renamed         ('R')  -> hint               (glyph + filename)
Conflicted      ('!')  -> conflict bold      (glyph + filename)
Ignored         (' ')  -> disabled           (glyph hidden, filename dim)
Clean tracked   (' ')  -> default            (no tint)
```

Filetype color (from
[TASK:phase-9/fm-filetype-colors](spec:TASK:phase-9/fm-filetype-colors))
wins when git status is clean; git status wins otherwise. The
ordering is one `match` arm in the row renderer — no separate
priority machinery.

## Verification

- Create a dirty working tree (modify, stage, delete, untracked):
  rows tint as above.
- Conflict marker shows bright magenta `!`.
- Filetype colors are visible on clean files (the git tint doesn't
  bleed across rows).

## Where it slots in

- Edit: `crates/file-manager/src/file_manager.rs` —
  `git_status_decoration` → `git_status_palette` (signature change,
  ~30 LOC).
- Edit: `crates/file-manager/src/view.rs` row renderer — single
  consumer.
