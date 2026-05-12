---
id: TASK:phase-5/goto-or-open-pane
type: task
status: accepted
version: 0.0.1
summary: >
  `GotoOrOpen{Terminal,FileManager,Editor}` actions that focus the
  most-recently-active pane of the requested kind in the current
  window, or split / open a new one when none exists.
owners: [carlo]
progress: done
refines:
  - REQ:codon/pane-ux#c-goto-or-new
---

# Goto-or-open pane actions

## What ships

Three new actions registered in `codon-session` (or `codon-keymap` —
wherever the cross-cutting action verbs live):

- `GotoOrOpenTerminal`
- `GotoOrOpenFileManager`
- `GotoOrOpenEditor`

Each one walks the current window's pane tree:

1. If any pane in the tree contains an item of the requested kind,
   focus the most-recently-active one (tracked via item
   focus-in-time, fallback to first match).
2. Otherwise, split the active pane (default direction: right;
   honours user-configured `default_split_direction`) and open a new
   item of the requested kind — for terminal: `workspace::NewTerminal`;
   for file manager: the existing FM open action; for editor: a
   blank buffer (`workspace::NewFile`).

Default keybindings (in the embedded keymap and
`assets/config/codon.example.toml`):

- `cmd-t` → `GotoOrOpenTerminal`
- `cmd-e` → `GotoOrOpenFileManager`
- `cmd-shift-e` → `GotoOrOpenEditor`

## Why this shape

Today the user has to remember whether a terminal already exists
before pressing `cmd-shift-t` (new) vs hunting for the existing one
with `ctrl-hjkl`. Single-chord deterministic entry per kind
collapses that to "press the key, get a terminal" regardless of
state — same ergonomics as Helm-mode in Emacs or `<leader>t` in
NeoVim configs.

## Reference points

- `Workspace::panes()` and `Pane::items()` give the iteration.
  Pane-kind detection: terminal items already implement a
  `Terminal`-shaped `Item` (search for `TerminalView::for_item`);
  file-manager has its own `FileManagerView` item; editor is any
  `editor::Editor` instance.
- Focus-time tracking can piggy-back on existing item focus events
  (`Pane::activate_item`), stored in a session-scoped HashMap keyed
  by `EntityId`.

Effort: medium. ~150–200 LOC for the walk + dispatch logic, mostly
boilerplate for the three pane kinds.
