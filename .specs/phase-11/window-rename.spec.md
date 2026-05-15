---
id: TASK:phase-11/window-rename
type: task
status: accepted
version: 0.1.0
summary: >
  WindowRename — single-line text-input modal to rename the active
  window (tmux `prefix ,`).
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-rename
categorized_under: [TOPIC:topics/phase-11]
---

# Window rename

## What ships

- New action `codon_session::WindowRename`.
- New modal `WindowRenameModal` (`window_rename.rs`). One-line
  `Editor` seeded with the current window name, focused on open.
  `enter` commits, `esc` / blur cancels.
- On commit: update `Session.windows[active].name`, `upsert` into
  the registry, `persist_async`, `cx.notify()` so the status-bar
  tab strip re-renders.
- Empty / whitespace-only input cancels without mutation. Same
  behavior as a blur — no error toast, no log spam.
- Duplicate names within the session are allowed; collision
  disambiguation is the status-bar's job (clause `#c-rename`
  describes the `#id` suffix). For now, the simplest implementation
  is to always show the id-suffix when two adjacent tabs in the
  TabBar share a name; can be tightened later.
- Bound under both the menu (`cmd-k shift-w r`) and the 2-key path
  (`cmd-k r`). `cmd-k r` is currently unbound.

## Why this shape

Reuses the upstream `Editor` widget rather than rolling a fresh text
input. The `Picker` modal is overkill — there's no candidate list
and no fuzzy match. A dedicated modal is the right footprint.

Effort: medium. ~120 LOC for the modal + handler + keymap.
