---
id: TASK:phase-7/fm-search-by-content
type: task
status: accepted
version: 0.0.1
summary: >
  `S` opens a ripgrep-backed content-search picker. Results show
  `path:line: snippet`; Enter opens the file at the line.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-find-search#c-search-by-content
---

# File-manager search-by-content

## What ships

`S` (shift-s) opens a `Picker` that runs `rg --json <query>`
rooted at `current_dir` and parses the streaming output. Each
result row shows `<relative-path>:<line>:<col>  <snippet>` with
the matched substring highlighted.

Enter opens the file via `workspace.open_abs_path` with a
position hint (`{line, col}` — Zed's `OpenOptions` supports
this).

No `ripgrep` binary → toast "Install ripgrep for content search"
and dismiss. No `walkdir` fallback (would be too slow and unranked
to be useful at scale).

## Approach

- Reuse the search module from TASK:phase-7/fm-search-by-name.
- Spawn `rg` with `--json --max-count=50 --max-columns=200` to
  keep memory bounded; chunk parsed entries into the picker.
- Snippet highlighting: rg's JSON includes `submatches` with
  `start` / `end` byte offsets — use those.

~200 LOC.
