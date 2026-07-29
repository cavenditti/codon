---
id: REQ:codon/fm-ranger-keybindings
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Ranger-compatible file-manager bindings integrated with Codon's
  keymap discovery surfaces.
owners: [carlo]
refines: []
categorized_under: []
---

# Ranger-compatible file-manager keybindings

## Context

Codon's integrated file manager already shares Ranger's Miller-column
navigation model, but its bindings grew feature-by-feature in raw key
handling. The user's effective Ranger 1.9.4 map should be available as
muscle-memory aliases without hiding those aliases from the keymap
cheatsheet or status-bar glance. Where Ranger's terminal UI vocabulary
conflicts with Codon's Helix layer, Codon keeps its native verb and adds
the closest safe Ranger alias documented by this requirement.

:::{requirement id="fm-ranger-keybindings" level="MUST"}
The file manager MUST expose Ranger 1.9.4 browser-mode navigation,
history, bookmarks, marking, file operations, sorting, filtering,
search, goto, tab, and function-key bindings wherever Codon has an
equivalent verb. These bindings MUST be declared through Codon's TOML
keymap/action registry so `codon_keymap::ShowKeymap` lists them.

Codon MUST retain the approved Helix adaptations: Space remains the
global leader; `y`/`d`/`p` remain immediate single-key verbs, with
conflicting Ranger sub-chords moved under `g`; `:` remains the Codon
palette; `a`/`A`, `r`, `s`/`S`,
and Ctrl-R retain their Codon meanings; shell prompts remain `;`/`!`;
Ranger `q`/`Q`/`ZZ`/`ZQ` use Codon's safe-close action; Ranger tabs map
to pane-item tabs; and Ranger console/pager/task-view internals keep
their native Codon controls.

The user's Ranger override MUST be preserved: `F` opens the filter
prompt, while follow-symlink moves to `g f` and `f` opens find.

The file-manager status glance MUST advertise the Ranger compatibility
surface and every added binding MUST have automated parsing/discovery
coverage. Unsupported Ranger-only verbs MUST fail by omission rather
than dispatching a misleading substitute.
:::
