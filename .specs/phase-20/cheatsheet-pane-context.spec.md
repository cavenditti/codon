---
id: TASK:phase-20/cheatsheet-pane-context
type: task
status: draft
version: 0.0.1
summary: >
  Make `prefix <F1>` (`codon_keymap::ShowKeymap`) open with the
  currently focused pane's tab pre-selected and render sections in
  the order *global → focused pane → other panes alphabetised*.
  Re-invoking from a different pane re-selects the new pane's tab.
  Answers "what can I do here" before "what can I do anywhere."
owners: [carlo]
progress: done
refines:
  - REQ:codon/discoverability#c-cheatsheet-pane-context
---

# Pane-aware cheatsheet

## Plan

### Today's shape

The cheatsheet lives in
[`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)
and exposes a fixed tab order via `KeymapCheatTab` (e.g. Global,
Editor, Terminal, FileManager, …). The initial tab is hard-coded
to whichever tab the enum lists first.

### Target shape

When `codon_keymap::ShowKeymap` opens the modal:

1. Read the focused pane's `PaneKind` from the global
   `CodonModeTracker`
   ([`crates/codon-pane-bridge/`](spec:src:crates/codon-pane-bridge)).
2. Map `PaneKind` → `KeymapCheatTab`:
   - `Editor` → `KeymapCheatTab::Editor`
   - `Terminal` → `KeymapCheatTab::Terminal`
   - `FileManager` → `KeymapCheatTab::FileManager`
   - `GitPanel` → `KeymapCheatTab::GitPanel`
   - others → `KeymapCheatTab::Global` (fallback)
3. Open the modal with that tab pre-selected (replacing the
   current fixed default).
4. Re-order tab rendering left → right as:
   `Global → <focused pane> → <other panes alphabetised>`.
   Tabs the keymap registry has no bindings for are hidden (no
   change from current behaviour).

### Re-invocation

If the cheatsheet is already open and the user re-invokes
`ShowKeymap` after switching panes, the modal's open() path
should re-read the focused pane and re-select the new pane's tab.
The simplest mechanism: dismiss + re-open. Tests cover the
re-select behaviour.

### Section ordering inside a tab

Each tab today renders bindings in the order they appear in the
embedded TOML / user TOML. Keep that — phase 20's chord-rename
work already produces a sane order. Two improvements that are
in-scope if cheap:

- Group bindings by their first chord head (`prefix w …`,
  `prefix a …`, `space …`) so related verbs cluster visually.
- Render an "Unbound" group at the bottom listing actions that
  exist in the codon-side registry but have no chord — visible
  inventory for "what could I rebind."

If either is non-trivial, defer to a follow-up task.

### Global collapse

The Global tab dominates the listing (~50 bindings). It MUST be
collapsible — a chevron or `tab`-to-toggle interaction — and MUST
default to expanded so first-time users still see it.

## Acceptance

- Open the cheatsheet from a focused editor — Editor tab is
  pre-selected, Global tab visible to the left.
- Open from a focused terminal — Terminal tab pre-selected.
- Open from a focused file manager — FileManager tab pre-selected.
- Open from an empty pane / unrecognised focus — Global tab
  pre-selected (no crash).
- Re-invoke after switching panes — new pane's tab is selected.
- Global tab collapses on `tab` keypress (or whatever interaction
  the modal uses for tab navigation; pick one and document).
- `spec lint` clean.

## Files touched

- `crates/codon-keymap/src/cheatsheet_modal.rs` — open path reads
  the mode tracker, sets initial tab, reorders rendering.
- `crates/codon-pane-bridge/src/` — possibly expose a helper
  if the focused-pane-kind lookup isn't already there.
- Tests covering each pane-kind → tab mapping + the fallback.
