---
id: TASK:phase-22/command-history-search-and-picker
type: task
status: accepted
version: 0.1.0
summary: >
  Implement the `HistoryPicker` modal (bound to `prefix h`) and the
  underlying search-store API. The picker pastes the raw command at
  the terminal's PTY cursor (no execute), yanks the raw command
  with `y`, and opens a detail view with `e`. This is a local-user
  surface — no redaction on this path.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-history#c-picker
aspects: [picker-modal]
blocked_by:
  - TASK:phase-22/command-history-store
---

# History picker

## Plan

- New module `crates/codon-command-history/src/picker.rs` built on
  [`codon-pickers::ModalScaffold`](spec:src:crates/codon-pickers/src/scaffold.rs)
  + Zed's `picker::Picker`.
- Row rendering: `<relative-time> <cwd-tail>  <summary_what>`.
  When `summary_what` is NULL (llm_skipped — either by budget or
  risky-redaction), fall back to the raw `command_text`
  (truncated to ~80 cols with trailing `…`).
- Bindings inside the picker:
  - `enter` — paste the raw command bytes at the focused
    terminal's PTY cursor (same pathway as
    `contextual-suggest-terminal-shape`'s prefill — no trailing
    `\n`). The user typed these bytes once before; pasting them
    back is no different from `↑` in the shell.
  - `y` — yank the raw command to the clipboard.
  - `e` — open a detail view (read-only buffer) with the full
    raw output excerpt + both summaries + tags. If summaries
    are NULL the detail view shows the raw output excerpt and a
    one-line banner explaining why (budget exhausted /
    risky-redaction).
  - `space` — toggle pinning. Pinned entries float to the top
    on next open. Pin is stored in a separate column added in
    sibling task `command-history-store` (add `pinned INTEGER
    NOT NULL DEFAULT 0`).
- Add `"prefix h" = "codon_command_history::OpenPicker"` to the
  embedded keymap. Confirm collision-free first; rebind if `cmd-k
  h` is already taken.
- Search input wired to `HistoryStore::search { query, cwd?,
  limit? }`. Default `cwd = None`; the picker shows a header
  toggle (a `Tab` key) to scope to "this cwd only" — useful when
  the user is in a known directory.

## Acceptance

- `cmd-k h` from a terminal opens the picker with the workspace's
  entries.
- Enter pastes the raw command at the PTY cursor (verified via
  captured PTY writes) — same for clean and llm_skipped rows.
- `y` round-trips the raw command through the clipboard.
- `e` opens a read-only detail view with the summaries (or the
  raw output excerpt + skip-reason banner if `llm_skipped`).
- `cargo test -p codon-command-history` passes.
