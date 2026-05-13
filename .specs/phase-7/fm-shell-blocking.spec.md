---
id: TASK:phase-7/fm-shell-blocking
type: task
status: accepted
version: 0.0.1
summary: >
  `!` prompts for a shell command; on Enter the FM grays out and the
  command runs in the chosen terminal pane until exit.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-shell-exec#c-shell-blocking
---

# File-manager shell exec — blocking (`!`)

## What ships

`!` opens an Insert-mode prompt seeded with `:` (chord open).
Enter applies substitutions
(REQ:codon/fm-shell-exec#c-shell-substitutions) and runs the
command in the terminal pane chosen by
TASK:phase-7/fm-shell-terminal-reuse.

While running, the FM pane visually grays (translucent overlay
with the running command + a small spinner). Esc on the overlay
sends SIGTERM to the process group; a second Esc forces SIGKILL.

On non-zero exit, the captured stderr surfaces via
`surface_error` (see TASK:phase-7/fm-shell-stderr-toast).

## Where it slots in

- Add `PendingInput::ShellBlocking { input }`.
- Spawn a terminal item via `terminal_view::TerminalView::new`
  with the resolved command; subscribe to its exit event.
- ~250 LOC including overlay + signal handling.
