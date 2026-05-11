---
id: TASK:phase-5/window-overview
type: task
status: accepted
version: 0.0.1
summary: >
  Tmux-style window overview within the active session — each window
  as a thumbnail tile (name, dominant pane kind, layout sketch), hjkl
  navigation, Enter to switch.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/windows#c-overview
---

# Window overview

## What ships

New action `codon_session::WindowOverview` that opens a modal grid
showing every window in the active session. Each tile:

- Window name (large)
- Dominant pane kind icon (terminal / editor / file-manager / agent)
- A small layout sketch — drawn from the cached
  `LayoutSnapshot::Member` tree as nested rectangles, so the user
  can recognise their split shape at a glance.
- Active-window highlight (accent border).

Navigation matches the session overview: `h`/`j`/`k`/`l` move the
highlight, `Enter` swaps to the chosen window via the same layout
swap path `WindowGoto` uses today, `Esc` dismisses, `/` filters
fuzzy.

## Why this shape

`WindowSwitch` is a list — fine for many windows. Overview is a
spatial view — fine for "I had a 3-pane git diff window somewhere,
which one was it?". Sketching the layout makes the visual
recognition cheap.

## Reference points

- The `LayoutSnapshot` types in
  [`vendor/zed/crates/workspace/src/codon_bridge.rs`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs)
  already encode the split tree; the sketch just walks it and
  paints rectangles proportional to each leaf's relative size.
- Pane-kind detection: the `Member::Pane` variant carries enough to
  pick an icon (terminal vs editor vs file-manager) by inspecting
  the dominant item kind.

Effort: medium-large. ~300 LOC — the layout-sketch widget is the
new bit, the rest mirrors session-overview.
