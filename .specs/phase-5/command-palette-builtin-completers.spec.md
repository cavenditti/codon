---
id: TASK:phase-5/command-palette-builtin-completers
type: task
status: accepted
version: 0.0.1
summary: >
  Built-in Completer impls for the high-traffic verbs: file paths,
  theme names, line numbers, and free-text search patterns.
owners: [carlo]
progress: done
refines:
  - REQ:codon/command-palette#c-builtin-completers
---

# Built-in completers

## What ships

Four `Completer` implementations in
`crates/codon-command-palette/src/completers/`:

| File | Registered for | Items |
|---|---|---|
| `file_path.rs` | `workspace::Open`, `editor::OpenFile` | files reachable from the active project, ranked via `fuzzy::match_strings` against the active worktree's file paths |
| `theme.rs` | `theme_selector::Toggle` | every installed theme name from the global `ThemeRegistry` |
| `line_number.rs` | `editor::GoToLine` | numeric-only; produces a single item `<n>` for any parse-able integer in the query, no list expansion needed |
| `search.rs` | `workspace::NewSearch` | free-text passthrough — items are the user's query verbatim (one item); confirms the existing fallback behaviour but with the description pane explaining "free-text query" |

Each is registered at crate `init` time via the registry from
`command-palette-completer-trait`.

## Reference points

- [`vendor/zed/crates/file_finder/src/file_finder.rs`](spec:src:vendor/zed/crates/file_finder/src/file_finder.rs)
  — file-path fuzzy matching; the same code path (worktree
  enumeration + `fuzzy::match_strings`) feeds the `file_path`
  completer.
- [`vendor/zed/crates/theme_selector/src/theme_selector.rs`](spec:src:vendor/zed/crates/theme_selector/src/theme_selector.rs)
  — theme enumeration via `ThemeRegistry`.
- [`vendor/zed/crates/go_to_line/src/go_to_line.rs`](spec:src:vendor/zed/crates/go_to_line/src/go_to_line.rs)
  — current line-number prompt — same shape, different chrome.

## Tests

- Unit: `file_path::complete("READ", cx)` ranks README first when
  README is in the worktree.
- Unit: `theme::complete("dark")` returns at least the bundled dark
  themes.
- Unit: `line_number::complete("42")` produces one item with
  value `"42"`.

Effort: medium. ~250 LOC across the four files.
