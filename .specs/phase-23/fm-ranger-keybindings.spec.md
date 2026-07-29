---
id: TASK:phase-23/fm-ranger-keybindings
type: task
status: accepted
version: 0.1.0
summary: >
  Add Ranger-compatible FM aliases through the central keymap and
  discoverability pipeline.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-ranger-keybindings#fm-ranger-keybindings
assignee:
eta:
blocked_by: []
---

# Ranger-compatible file-manager keybindings

## Plan

Inventory the user's effective Ranger 1.9.4 browser map, add any missing
file-manager actions/handlers, and declare compatible bindings in the
embedded default TOML. Preserve the approved Helix deviations in
`FileManager::handle_key_down`, update the FM glance, and keep the
cheatsheet driven by the same central binding table.

## Acceptance

- Ranger navigation, history, bookmark, mark, operation, sort, filter,
  find, goto, safe-close, tab, and function-key aliases dispatch against
  a focused `FileManager`.
- `F` filters, `f` finds, and `g f` follows a symlink.
- Single-key Codon verbs and the Space leader retain their behavior.
- Ranger sub-chords do not make `y`, `d`, or `p` wait for the global
  chord timeout.
- The default keymap parser and cheatsheet tests enumerate representative
  Ranger aliases, including multi-key chords.
- File-manager and codon-keymap focused tests pass; `spec lint` is clean.
