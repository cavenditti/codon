---
id: TOPIC:topics/phase-20
type: topic
status: draft
version: 0.0.1
summary: >
  Ease of use and discoverability — collapse fragmented verbs, repair
  anti-mnemonic chords, and add the runtime affordances (action history,
  status-bar mode glance, binding hints, dead-end flash, pane-aware
  cheatsheet) so the user can perform most actions without memorising
  the chord map.
owners: [carlo]
---

# Phase 20 — Ease of use and discoverability

Phases 1–19 grew the codon action surface to ~120 bound chords across
six pane kinds. The surface area is now large enough that "remember
the chord" stops being the right primary affordance. Phase 20's goal
is to shrink what the user has to memorise to a small, mnemonic core
while making everything else discoverable at the moment of use.

Two attack angles:

1. **Vocabulary** — collapse verbs that fragment by pane kind into
   single context-aware actions, and rename a small set of
   anti-mnemonic chords (`prefix w` for close, `prefix p` overloaded
   as both window-prev and picker sub-prefix, `prefix l` overlapping
   with `ctrl-l` pane focus). Captured in
   [REQ:codon/keymap-vocabulary](spec:REQ:codon/keymap-vocabulary).

2. **Discoverability at use time** — add five runtime affordances
   that teach the keymap without the user opening anything:
   - `.` repeats the last non-motion action plus a `prefix ;` picker
     of the last ~10, so the palette becomes a teaching surface
     rather than a memorisation tax.
   - A 2-second status-bar glance on every mode transition listing
     the highest-frequency verbs for that mode.
   - Bound-chord rendering everywhere a verb appears in UI (palette,
     menus, cheatsheet, peek footer).
   - A 200 ms status-bar colour flash on chord dead-end / timeout
     in place of any potential toast.
   - The cheatsheet (`prefix <F1>`) opens with the focused pane's
     section pre-selected and surfaces global → focused → other
     panes, so it answers "what can I do here" first.

   Captured in [REQ:codon/discoverability](spec:REQ:codon/discoverability).

The which-key overlay shipped earlier already covers the chord-tree
prompt and is intentionally untouched in this phase.

Refining requirements:

- [REQ:codon/keymap-vocabulary](spec:REQ:codon/keymap-vocabulary) —
  verb collapse (split / open-or-focus / close), chord renames
  (`prefix w` and `prefix shift-w` swap, `prefix l` drop, `prefix
  shift-t/e` for always-new), the global `space`-leader picker
  flow that replaces `prefix p X`, and the FM `.` → `, h` move
  that frees `.` for the action-history repeat.
- [REQ:codon/discoverability](spec:REQ:codon/discoverability) —
  action-history ring, status-bar mode glance, binding-hints-
  everywhere audit, dead-end colour flash, pane-aware cheatsheet.
