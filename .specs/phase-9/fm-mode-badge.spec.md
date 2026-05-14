---
id: TASK:phase-9/fm-mode-badge
type: task
status: accepted
version: 0.0.1
summary: >
  Render a small colored badge in the file-manager footer reflecting
  the current `CodonModeTracker` state — Normal green, Insert blue,
  Visual orange. Updates live as focus shifts.
owners: [carlo]
progress: done
refines:
  - REQ:codon/file-manager-theme#c-mode-badge
---

# File-manager mode badge

## What ships

A `div().bg(...).px_2().rounded_sm()` chip rendered at the left of
the footer row in `view.rs::render_status_bar` (or equivalent). The
chip reads from `CodonModeTracker::active_pane_mode(cx)` and maps:

```
Normal  -> success bg, "NOR" label
Insert  -> info bg,    "INS" label
Command -> warning bg, "CMD" label
```

When the file manager hosts a transient prompt (delete confirm,
rename input, etc.) the tracker already reports `Insert` — no
extra state machine. When the row list owns focus, the tracker
reports `Normal`.

`cx.observe_global::<CodonModeTracker>` triggers re-render on
state change.

## Verification

- Open fm: badge shows `NOR` green.
- Press `r` to rename: badge flips to `INS` blue while the prompt
  is open, back to `NOR` on Esc / Enter.
- Press `:` to open the palette: badge stays `NOR` (palette is a
  modal layered above, not an fm prompt).

## Where it slots in

- Edit: `crates/file-manager/src/view.rs` — ~40 LOC in the footer
  block, plus a `cx.observe_global` subscription in the panel
  constructor.
