---
id: TASK:phase-11/break-pane-to-window
type: task
status: accepted
version: 0.1.0
summary: >
  BreakPaneToWindow — promote the active pane in the current window
  into a new window of its own (tmux `prefix !`).
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-break-pane
categorized_under: [TOPIC:topics/phase-11]
---

# Break pane to new window

## What ships

- New action `codon_session::BreakPaneToWindow`.
- New helper `swap::split_off_active(snapshot) -> (remaining, broken)`
  that walks a `LayoutSnapshot`, finds the pane marked `active: true`,
  and returns:
  - `broken` — a single-pane `LayoutSnapshot` containing just that
    pane (re-flagged `active: true` for the new window).
  - `remaining` — the original tree with that pane removed and
    Group nodes collapsed when they degenerate to a single child.
  If the active flag isn't set on any pane (e.g. a freshly-applied
  snapshot before any pane has been focused), fall back to the
  first pane in document order. If the window has only one pane,
  the action is a no-op with a brief toast ("Window already has
  only one pane").
- The handler:
  1. Capture current center layout via `swap::capture`.
  2. Split into `(remaining, broken)`.
  3. Replace center with `remaining` (so the current window now
     shows the smaller layout).
  4. Persist a new `Window` whose `layout = Some(broken)`, append
     it to the session, mark it the new active window, and call
     `switch_to_window` so the runtime-cache plumbing kicks in.
  Items rehydrate via `SerializableItemRegistry` — the
  `codon_bridge` docstring explicitly guarantees this for buffers
  and terminal connections.
- Bound under both the menu (`cmd-k shift-w !`) and the 2-key path
  (`cmd-k !`). `cmd-k !` is currently unbound.

## Why this shape

Doing the split at the `LayoutSnapshot` level keeps the work inside
`codon-session`: no new public surface needed on
`workspace::codon_bridge`. The serializable-item-id guarantee means
we don't lose terminal state.

The "collapse degenerate Group" step matters: if you break the
right half of a 2-pane horizontal split, the remaining `Group` has
one child, which `apply_layout`'s pane-group deserializer treats as
a valid (but odd) shape. Collapsing it to the child snapshot keeps
the resulting layout tidy.

Effort: medium. ~80 LOC of pure-function snapshot manipulation plus
a handler.
