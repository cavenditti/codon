---
id: TASK:phase-9/fm-cursor-contrast
type: task
status: accepted
version: 0.0.1
summary: >
  Bump cursor-row background to `ghost_element_active` and bold the
  filename. Row must remain readable when marked + cursor coincide.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/file-manager-theme#c-cursor-row-contrast
---

# File-manager cursor-row contrast

## What ships

Two changes in the row renderer:

1. Cursor-row background: swap `ghost_element_selected`-ish low
   alpha for `ghost_element_active` (or
   `elevated_surface_background` if the active token reads too
   dark — pick whichever has the higher contrast against panel
   bg). Single line change.
2. Filename `Label` adds `.weight(FontWeight::BOLD)` on the
   cursor row.

When marked + cursor coincide: the marked stripe
([TASK:phase-9/fm-marked-stripe](spec:TASK:phase-9/fm-marked-stripe))
stays visible because it's a left-edge element, and the bold
filename plus stronger bg makes the row pop.

## Verification

- Move cursor through the list: the active row is clearly bolder
  and more saturated than its neighbors at a glance from 2m away.
- Mark cursor row: stripe visible, bg still contrasts with
  un-marked unselected rows.

## Where it slots in

- Edit: `crates/file-manager/src/view.rs` — ~5 LOC.
