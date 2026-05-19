---
id: REQ:codon/stacked-panes
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  First-class stacked panes — add `Member::Stack` to the
  vendored `workspace::pane_group`, render it live with a
  no-close-X tab strip, wire `pane.stack.{cycle,add,remove,
  promote}` actions, and stop degrading `LayoutSnapshot::Stack`
  to active-member-only on apply. Today's tabs cover the
  common case; this lifts the "stacks deferred" footnote in
  REQ:codon/layout into a real feature when nested layouts
  become routine.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-19]
---

# First-class stacked panes

## Context

The original v0 design described three layout node kinds —
`Split`, `Stack`, `Leaf` — with Stack as the tabs-replacement:
N panes in the same slot, one visible at a time, cycle visibility
with a binding. Phase 2 shipped `LayoutSnapshot::Stack` as a
round-tripped serde variant but left live rendering deferred
behind `REQ:codon/layout#c-stack-live-rendering`:

> first-class Stack rendering is deferred (needs new
> `Member::Stack` in the vendored pane_group, which has 15+
> match-site impacts)

Today the windows-within-session model (Phase 2 + Phase 11)
covers most of the use case — switching windows is the tabs
analogue. Stacks remain useful for nested layouts: e.g. a
three-pane vertical split where the middle pane holds a stack of
"git status / git log / git blame" mini-views the user cycles
without breaking the outer layout. Low priority, but the gap is
worth closing as a coherent unit rather than incrementally.

:::{requirement id="stacked-panes" level="SHOULD"}
The system MUST provide:

- {#c-member-stack} a new `Member::Stack { panes: Vec<PaneId>,
  active: usize }` variant in
  `vendor/zed/crates/workspace/src/pane_group.rs`, sibling to
  the existing `Member::Pane` and `Member::Axis` variants.
  Storage owns the ordered pane list; `active` is the index of
  the visible pane. Empty stacks are not representable — adding
  the last pane out of a stack collapses it back to a `Member::Pane`.

- {#c-match-site-coverage} every match-site on `Member` in the
  vendored workspace tree handles the `Stack` arm explicitly
  (no `_ =>` wildcards that silently ignore it). The full set
  was estimated at ~15 sites; the implementing task walks them
  end-to-end. Sites that genuinely don't care about Stack-vs-Pane
  delegate to a `Member::active_pane()` helper that returns the
  visible pane.

- {#c-render} live rendering: the pane host renders only the
  stack's active member, with a no-close-X tab strip above it
  (one tab per stack member, active tab highlighted). The strip
  reuses `ui::TabBar` with `Tab::end_slot(None)` — same shape
  as the windows status-bar indicator from
  [REQ:codon/windows#c-status-bar](spec:REQ:codon/windows#c-status-bar).
  Click-to-switch on tabs is allowed (mirroring the windows
  indicator); keyboard verbs are the primary path.

- {#c-actions} four new actions in `codon-session`:

  - `codon_session::PaneStackCycle(direction)` — cycle the
    active stack member forward or back. Default bindings:
    `prefix shift-tab` cycle back, `prefix tab` cycle forward.
  - `codon_session::PaneStackAdd(kind)` — add a new pane of the
    given kind to the active stack (creates a new stack from
    the current leaf if none exists). Default binding via the
    command palette only — no chord shortcut.
  - `codon_session::PaneStackRemove` — remove the active stack
    member; collapse the stack to `Member::Pane` if only one
    pane remains.
  - `codon_session::PaneStackPromote` — extract the active
    stack member into its own split sibling. Inverse of
    `PaneStackAdd`. Default binding via the palette.

- {#c-layout-snapshot} `LayoutSnapshot::Stack` round-trips
  through `capture_layout` / `apply_layout` *without*
  degrading to the active member on apply. The existing
  `c-stack-fallback` clause in `REQ:codon/layout` is satisfied
  by this REQ. Old saved sessions that captured a stack as
  "active member only" still deserialize (forward-compatible
  fallback path stays in place).

- {#c-persistence} per-stack `active` index is persisted as part
  of the LayoutSnapshot. On rehydrate, the originally-active
  member is shown; other members are constructed lazily on
  first cycle to keep restart cheap.

- {#c-keymap-defaults} default bindings live in the embedded
  TOML under `[bindings.normal]`:

  ```toml
  "prefix tab"       = "codon_session::PaneStackCycle(forward)"
  "prefix shift-tab" = "codon_session::PaneStackCycle(back)"
  ```

  No `prefix s` overload — `prefix s` already maps to
  `SessionOverview`. The two cycle bindings plus the palette
  cover the day-to-day; less-common verbs (`Add`, `Promote`,
  `Remove`) are palette-only by default to keep the chord space
  uncluttered.

- {#c-overview-shorthand} the session / window overview rows
  show a stack-count indicator (e.g. `≡3`) alongside the
  existing layout shorthand from
  [REQ:codon/windows#c-overview](spec:REQ:codon/windows#c-overview),
  so a stack-heavy layout is visible at a glance.
:::

## Coordination

This REQ touches `vendor/zed/` in two non-trivial ways:

1. The `Member::Stack` variant in `pane_group.rs` is additive
   but every Zed call-site on `Member` must compile. Upstream
   PRs against vendored Zed follow the project's standard flow
   (commit on `codon` branch, `./script/clippy`, then bump the
   submodule pointer in the outer repo).
2. The tab-strip rendering reuses `ui::TabBar`; no new UI crate
   is needed.

## Out of scope

- Floating / pinned stacks. Stacks live as leaves of the
  `Member` tree, same positional rules as splits.
- Drag-to-reorder stack members. Keyboard verbs only.
- Per-stack scrollback / state isolation beyond what each pane
  kind already provides.
