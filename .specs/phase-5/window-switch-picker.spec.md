---
id: TASK:phase-5/window-switch-picker
type: task
status: accepted
version: 0.0.1
summary: >
  Fuzzy picker (`WindowSwitch`) for jumping by name across the active
  session's windows — same shape as `SessionSwitch` but scoped to one
  session.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/windows#c-switch-picker
---

# Window switch picker

## What ships

New action `codon_session::WindowSwitch` that opens a `picker::Picker`
listing every window in the active session by name. Selecting one
calls the same swap-on-switch path as `WindowGoto(usize)` does today.

- Picker delegate mirrors the `SessionSwitch` delegate at
  [`crates/codon-session/src/picker.rs`](spec:src:crates/codon-session/src/picker.rs).
- Match scoring via `fuzzy::match_strings`, the same call the file
  manager and command-palette use.
- Default keybinding: `cmd-shift-w` is taken by the OS-window-close
  chord — pick something like `cmd-shift-j` or `ctrl-w w` and add it
  to both the embedded keymap and `assets/config/codon.example.toml`.

## Why this shape

The window tab bar (`REQ:codon/windows#c-status-bar`) is fine for
sessions with 2–4 windows, but a keyboard-first user with a dozen
named windows wants fuzzy search, not 12 tabs to eyeball.
`SessionSwitch` already proves the pattern; this is the
window-scoped variant.

Effort: small. ~80–120 LOC reusing the existing picker delegate.
