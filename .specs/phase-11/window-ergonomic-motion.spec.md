---
id: TASK:phase-11/window-ergonomic-motion
type: task
status: accepted
version: 0.1.0
summary: >
  2-key bindings for WindowNext / WindowPrev / WindowLast under
  the `cmd-k` leader.
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-ergonomic-motion
categorized_under: [TOPIC:topics/phase-11]
---

# Ergonomic 2-key window motion

## What ships

- `cmd-k n` → `WindowNext`
- `cmd-k p` → `WindowPrev`
- `cmd-k l` → `WindowLast` (see `TASK:phase-11/window-last`)

The existing 3-key menu chords (`cmd-k shift-w l`, `cmd-k shift-w h`,
`cmd-k shift-w shift-l`) stay — they remain the "windows menu" entry
point for discoverability and the cheatsheet groups them together.
The 2-key bindings are the muscle-memory path for users who already
know what they want.

## Conflict check

- `cmd-k n` is currently unbound.
- `cmd-k p` is currently unbound.
- `cmd-k l` today maps to `workspace::ActivatePaneRight`. That's a
  legacy back-compat duplicate of `ctrl-l` and the keymap comments
  call it out as such. Repurposing `cmd-k l` for `WindowLast` is
  the right move: `ctrl-l` continues to handle pane motion, and the
  `cmd-k` leader becomes "session / window verbs".

The cheatsheet's contextual section will pick up the new bindings
automatically; no extra glue.

Effort: trivial. Default-keymap edits + the `.example.toml` mirror.
