---
id: TASK:phase-9/fm-header-chips
type: task
status: accepted
version: 0.0.1
summary: >
  Compact colored chips in the column header showing active sort
  mode + direction, filter, find, hidden-visible. Chips disappear
  when their state is default.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/file-manager-theme#c-header-chips
---

# File-manager header chips

## What ships

A right-aligned chip row in the file-manager header. Each chip:
`div().bg(...).px_1p5().rounded_sm()` with a 1-line label.

Chips, in display order:

1. **Sort** — always shown, label = mode + direction arrow
   (`name ↓`, `mtime ↑`, `size ↓`, `ext`, `btime`, `nat`,
   `rand`). Color = accent. Default (`name ↓`) chip dimmer so
   it's still informative but not loud.
2. **Filter** — shown only when filter active. Label =
   `filter:<pattern>` truncated to ~20 chars. Color = warning.
3. **Find** — shown only when a find query is live (after `f`).
   Label = `find:<pattern> (N)` where N is the match count.
   Color = info.
4. **Hidden** — shown only when hidden files are visible.
   Label = `.`. Color = muted.

Source of truth:
- Sort: `FmPrefs::sort_mode`, `FmPrefs::sort_direction`.
- Filter: existing `filter_pattern: Option<String>` on the panel.
- Find: existing `last_find_pattern: Option<String>` +
  `find_match_count`.
- Hidden: existing `FmPrefs::show_hidden`.

`cx.notify()` on any state change already triggers re-render —
no new subscriptions needed.

## Verification

- Open fm: only the dim default `name ↓` sort chip is visible.
- `,m` → chip becomes saturated `mtime ↓`.
- `/foo` filter active → yellow `filter:foo` chip appears.
- `fbar` find → cyan `find:bar (3)` chip; `n`/`N` cycles, count
  stays accurate.
- `zh` show hidden → muted `.` chip appears.

## Where it slots in

- Edit: `crates/file-manager/src/view.rs` — ~80 LOC in the header
  block. Pure additive — no new state, all reads from existing
  panel fields.
