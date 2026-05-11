---
id: TASK:phase-5/command-palette-description-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Render an always-visible description block next to (or below) the
  active row in the command palette. Sourced from action doc comments,
  never a tooltip.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-palette#c-description-pane
---

# Always-visible description pane

## What ships

A two-column layout for the codon command palette:

- Left column (existing Zed behaviour): the picker — fuzzy query
  input on top, matching rows below.
- Right column (new): a static description panel ~360px wide that
  re-renders on every selection change with:
  - command name (humanized via `humanize_action_name`)
  - keystroke chord, if bound — rendered via
    `ui::KeyBinding::from_keystrokes`
  - doc comment text, pulled from the action via
    `Action::action_documentation()` (the same source Zed uses for
    hover tooltips today)
  - argument hint, when a `Completer` is registered for this
    command — e.g. "expects a file path", "expects a theme name"

On a narrow window the panel collapses below the row list instead of
beside it (single-column fallback).

## Reference points

- [`vendor/zed/crates/command_palette/src/command_palette.rs`](spec:src:vendor/zed/crates/command_palette/src/command_palette.rs)
  — current tooltip-based documentation surface; the codon panel
  reads from the same `Action::action_documentation` API.
- [`vendor/zed/crates/ui/src/components/keybinding.rs`](spec:src:vendor/zed/crates/ui/src/components/keybinding.rs)
  — `KeyBinding::from_keystrokes` for the chord render.
- [`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)
  — pattern for fixed-width docs alongside a list (the cheatsheet
  uses three columns; here we want two).

## Tests

- Manual: open palette, cycle rows with `down`/`up`, confirm the
  description panel updates with each selection.
- Manual: select a command bound to a chord (e.g. file manager
  toggle) — confirm the chord renders in the description panel.

Effort: medium. ~150 LOC for the two-column container + selection
observer.
