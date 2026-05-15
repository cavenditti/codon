---
id: REQ:codon/keyboard-only-ui
type: requirement
status: superseded
version: 0.0.1
level: SHOULD
summary: >
  Superseded placeholder — the "keyboard-first, no mouse-only
  affordances" rule is now stated canonically in `/CLAUDE.md` and
  enforced as a review convention rather than as a clause-bearing
  REQ. This file exists only to keep the historical commit d4ce1f5
  Spec-Ref trailer resolvable.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-14]
---

# Keyboard-only UI (superseded)

## Why this file exists

Commit `d4ce1f5` ("fix: route every prompt through the in-app
renderer") used a `Spec-Ref:` trailer pointing at
`REQ:codon/keyboard-only-ui` before the REQ was drafted. The
underlying discipline — strip mouse-only affordances, prefer the
in-app modal renderer over OS-native dialogs, bind every verb in
TOML — was codified as the **"Keyboard-first, always"** paragraph
near the top of `/CLAUDE.md` instead of as a standalone REQ.

That paragraph reads (as of 2026-05-15):

> Codon is driven entirely by the keyboard. Never add or preserve
> mouse-only affordances like tab close "x" buttons, hover-only
> icons, or click-to-do-anything controls when a keybinding already
> covers the action. When porting UI from Zed, strip those
> affordances and rely on the codon TOML keymap. If a verb has no
> binding yet, add the binding to the TOML defaults — do not fall
> back to leaving a mouse control in place.

## Where it actually lives in the graph

Concrete clauses that enforce this discipline are scattered across
existing REQs:

- [REQ:codon/in-app-pickers](spec:REQ:codon/in-app-pickers) — the
  prompt-renderer redirect addressed in commit d4ce1f5.
- [REQ:codon/pane-ux](spec:REQ:codon/pane-ux) — tab strip and
  mouse-affordance audits.
- [REQ:codon/unified-config](spec:REQ:codon/unified-config) — TOML
  bindings as the single source of truth for verbs.

## Resolution

This placeholder satisfies `R013` for the legacy trailer; no
follow-up work is planned under this id. New keyboard-first work
goes onto the relevant existing REQ above, not here.
