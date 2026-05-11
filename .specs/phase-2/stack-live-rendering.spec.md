---
id: TASK:phase-2/stack-live-rendering
type: task
status: accepted
version: 0.1.0
summary: >
  Member::Stack variant rendered live in the layout tree. Wontdo because
  Zed's existing tabs (multiple items in one Pane) cover the multi-content
  case, and codon's windows cover the multi-layout case — Stack would only
  add nested sub-window layouts, a niche workflow that doesn't justify the
  cross-cutting Member-enum change.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/layout#c-stack-live-rendering
---

# Stack live rendering (wontdo)

## What was originally scoped

A `Member::Stack` variant alongside `Pane` and `Axis` in the layout
tree at `vendor/zed/crates/workspace/src/pane_group.rs`, plus a
no-close-X tab strip and `pane::stack::Cycle` / `Add` / `Remove`
actions.

## Why we won't ship it

The use cases collapse into two existing mechanisms:

- **Multiple heterogeneous contents in one slot** — Zed tabs already
  handle this. `Pane::items: Vec<Box<dyn ItemHandle>>` accepts any
  mix of Editor / TerminalView / FileManager, each with its own focus
  handle and tab content. The tab bar at the top of a pane is the
  switcher.

- **Multiple split sub-layouts in one slot** — codon's windows handle
  this at the next level up. `cmd-k shift-w l/h` cycles between
  whole-workspace layouts; that's effectively the same UX as stacking
  compound layouts within a slot, just scoped to the entire window.

The only workflow Stack would unlock that neither of the above covers
is "two pre-arranged split layouts I bounce between *without*
affecting the rest of the window". That's a real but narrow case, and
the implementation cost (a new `Member` variant cascades into ~15
match sites across `pane.rs`, `workspace.rs`, and the persistence
model) is high relative to the benefit.

## What stays

The serde-level `LayoutSnapshot::Stack` variant remains in
`crates/codon-session/` and `vendor/zed/crates/workspace/src/codon_bridge.rs`
as forward-compatible no-op (apply-layout falls back to the active
member). If we ever change our minds, the persistence layer is ready;
only the live-rendering layer would need the cross-cutting changes.
