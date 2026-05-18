---
id: TASK:phase-16/pickers-changed-files
type: task
status: draft
version: 0.0.1
summary: >
  Implement `codon_pickers::ChangedFilesPicker` — a
  `picker::Picker` delegate over the project's git status (entries
  with non-`Unmodified` state). Confirming a row opens the file
  at its first changed hunk. Bound to `prefix p g`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/helix-pickers#c-changed-files-picker
---

# Changed-files picker

## What changes

The picker enumerates the project's tracked changes. Source of
truth is the same git data the git pane already reads — reuse the
`GitStore` / `RepositoryEntry` types from
[`vendor/zed/crates/git/`](spec:src:vendor/zed/crates/git/) rather
than shelling out.

Module layout:

- Add `crates/codon-pickers/src/changed_files.rs`.
- Action: `codon_pickers::ChangedFilesPicker`, registered via
  `actions!`.

Picker rows render with git status glyphs:

```
M  src/main.rs
A  crates/codon-pickers/src/jumplist.rs
?? .specs/phase-16/pickers-changed-files.spec.md
D  vendor/zed/assets/keymaps/vim.json
```

(Status letters mirror `git status --short`; format conventions
from `vendor/zed/crates/git/src/status.rs::FileStatus`.)

Filter: include `Modified`, `Added`, `Renamed`, `Deleted`,
`Untracked`, `Conflict`. Exclude `Unmodified` and `Ignored`.

Confirm:

- Open the file in the active pane (same path the file finder
  uses).
- On open, scroll to the first changed hunk. Zed exposes hunk
  enumeration via `buffer_diff::DiffHunk`
  ([`vendor/zed/crates/buffer_diff/`](spec:src:vendor/zed/crates/buffer_diff/));
  the existing `editor::GoToHunk` action lands at the first hunk
  in display order — fire it after the buffer loads.

Binding (added to
[`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)):

```toml
"prefix p g" = "codon_pickers::ChangedFilesPicker"
```

The git pane (`prefix g s` /
[`codon_panes::OpenGit`](spec:src:crates/codon-panes/)) stays the
right answer for staged / unstaged review with diffs. The picker
is for *fast nav* — "take me to the changed file I want to look at
right now."

## Why this clause

Helix's `space g` is one of the verbs codon's git-pane-centric
flow doesn't cover well: the git pane is a *workspace* — you open
it, browse, stage, unstage. The picker is a *jump* — you fuzzy-
match, hit enter, you're at the hunk. Both are useful; this task
covers the latter.

## Verification

- Open codon in a repo with several changed files. Press
  `cmd-k p g`. Picker lists every changed file with its status
  glyph.
- Type a partial filename → fuzzy match narrows the list.
- Confirm a row → file opens, cursor lands at the first hunk.
- Edit a file to add an untracked file → re-open picker; the new
  file appears.

## Done when

- `ChangedFilesPicker` action is registered and bound to
  `prefix p g`.
- Picker rows render `git status --short`-style glyphs.
- Confirm opens the file at the first hunk.
- An integration test under `crates/codon-pickers/src/tests.rs`
  (or `git_ui` adjacent tests) verifies the filter / row format
  against a fixture repo.
- `spec lint` is at zero errors.
