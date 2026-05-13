---
id: TASK:phase-5/session-overview
type: task
status: accepted
version: 0.0.1
summary: >
  Tmux-style session overview — every session as a tile in a grid
  with name, cwd, window count, last-attached time, keyboard nav,
  Enter to attach.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-overview
---

# Session overview

## What ships

New action `codon_session::SessionOverview` that opens a modal
filling the workspace with a uniform grid of session tiles. Each
tile shows:

- Session name (large, bold)
- Cwd (smaller, truncated middle)
- Window count
- Last-attached relative time ("2h ago")
- A visual hint if it's the active session (accent border)

Navigation:

- `h`/`j`/`k`/`l` (and arrow keys) move the highlight across the
  grid; `Enter` attaches the highlighted session via the same path
  `SessionSwitch` uses.
- `Esc` dismisses without changing sessions.
- `/` enters a fuzzy filter that narrows the grid in place.

## Why this shape

`SessionSwitch` is a vertical list; an overview lets the user *see*
the shape of their workspace (cwds, window counts) before picking,
which is what tmux's `prefix s` is good for. Same data, different
visual organisation.

## Reference points

- [`crates/codon-session/src/picker.rs`](spec:src:crates/codon-session/src/picker.rs)
  — the data model for sessions and the existing picker shape.
- The grid layout can be a plain `flex_wrap` of fixed-width tiles
  inside a `workspace::ModalView`; no GPUI grid primitive needed.

Effort: medium. ~200–300 LOC for the modal + tile rendering + grid
navigation. Filter sub-mode can ship in a follow-up.
