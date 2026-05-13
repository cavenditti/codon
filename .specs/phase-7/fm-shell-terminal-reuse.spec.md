---
id: TASK:phase-7/fm-shell-terminal-reuse
type: task
status: accepted
version: 0.0.1
summary: >
  Pick the terminal pane for `!` / `;` — most-recently-active terminal
  if idle, otherwise spawn a new one.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-shell-exec#c-shell-terminal-reuse
---

# File-manager shell-exec terminal selection

## What ships

A helper `pick_terminal_for_shell(workspace, cx) -> TerminalTarget`
that returns either:

- `TerminalTarget::Existing(pane, item_index)` — the
  most-recently-active terminal in the active window, only if its
  PTY is idle (no foreground process, prompt visible). Set
  `cwd` to FM's `current_dir` via a `cd` line sent on the PTY
  before the command.
- `TerminalTarget::New { pane_to_split: Option<Pane> }` —
  spawn a fresh terminal split.

## Idle detection

Idle = `Term::has_foreground_process() == false` in alacritty
terms, OR the PTY shell pid == its foreground pid. The alacritty
side is the source of truth; codon's terminal wraps it and
already tracks foreground process for its own status display
([`vendor/zed/crates/terminal/`](spec:src:vendor/zed/crates/terminal/)).

## Where it slots in

Likely a method on `Workspace` extension or a free function in
[`crates/file-manager/src/`](spec:src:crates/file-manager/src/),
called by the `!` / `;` handlers.
~120 LOC including the most-recently-active-terminal lookup that
mirrors `GotoOrOpenTerminal`.
