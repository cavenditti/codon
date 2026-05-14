---
id: TASK:phase-9/fm-marked-stripe
type: task
status: accepted
version: 0.0.1
summary: >
  Marked rows get a 2px left-edge stripe in the accent color, in
  addition to the existing background tint, so marked rows stay
  visible when the cursor moves away.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/file-manager-theme#c-marked-row-stripe
---

# File-manager marked-row stripe

## What ships

In the row element, prepend a `div().w(px(2.)).h_full().bg(...)`
when the entry is in `marked_paths`. Color = the codon-mode
"selection" accent (theme token, not hard-coded), so it stays
visible against both selected and unselected backgrounds.

The current full-row alpha tint stays (it's the "marked" cue at
distance); the stripe is the precise indicator that survives
when the row is also the cursor row (high-alpha cursor bg can
swallow the marked tint today).

## Verification

- Mark 3 rows with `v` (or Space), move cursor away: all three
  show the left stripe.
- Cursor lands on a marked row: stripe + cursor highlight both
  visible, no contrast loss.
- Unmark: stripe disappears within one frame.

## Where it slots in

- Edit: `crates/file-manager/src/view.rs` row renderer — ~15 LOC.
