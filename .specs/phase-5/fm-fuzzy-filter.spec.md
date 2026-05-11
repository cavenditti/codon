---
id: TASK:phase-5/fm-fuzzy-filter
type: task
status: accepted
version: 0.0.1
summary: >
  `/` in file manager Normal mode enters a filter Insert mode that
  narrows the current directory listing via fuzzy::match_strings.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-enhancements#c-fuzzy-filter
---

# Fuzzy filter in file manager

## What ships

In file-manager Normal mode, `/` enters a filter sub-mode. While the
filter is active:

- typed characters narrow the current directory listing
- `Enter` or `Esc` commits the filter and returns to Normal mode
- `Esc` from filter mode clears the filter

## Where it slots in

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  already has an Insert-mode input bar (used for create / rename). The
  same `pending_input: Option<...>` field gains a `Filter` variant.
- Filtering reuses `fuzzy::match_strings`
  (same call codon-session's picker uses —
  [`crates/codon-session/src/picker.rs`](spec:src:crates/codon-session/src/picker.rs)).

## Approach

Add `filter_query: String` to `FileManager`. When non-empty, the
render loop filters `self.entries` through fuzzy before display.
Status bar shows `filter: <query>` while active. ~50–80 LOC.
