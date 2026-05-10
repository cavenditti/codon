---
id: REQ:codon/layout
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Capture and apply the workspace's center pane group as a serializable
  snapshot, with keyboard-resizable separators.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-2]
---

# Layout snapshots and resize

## Context

Sessions and windows both need to round-trip a workspace's center pane
group across an in-process swap. Zed's existing `SerializedPaneGroup`
is `pub(crate)` and SQL-bound, so codon adds a thin serde-friendly
mirror in a new public bridge.

:::{requirement id="layout" level="MUST"}
The system MUST provide:

- {#c-snapshot-types} a serde-friendly `LayoutSnapshot` enum with
  `Group { axis, flexes, children }`, `Stack { members, active }`,
  `Pane(PaneSnapshot)` variants
- {#c-capture} a `capture_layout(&Workspace, ...)` helper that walks
  the live `Member` tree
- {#c-apply} a `Workspace::replace_center_with_snapshot` method that
  drops old panes after the new tree is built (preserving item ids so
  buffers and terminal cwds rehydrate)
- {#c-resize} keyboard-resizable separators bound to
  `cmd-k shift-{h,j,k,l}` via `vim::ResizePane*`
- {#c-stack-fallback} the `Stack` snapshot variant deserializes by
  falling back to the active member when applied — first-class Stack
  rendering is deferred (needs new `Member::Stack` in the vendored
  pane_group, which has 15+ match-site impacts)
- {#c-stack-live-rendering} live `Member::Stack` rendering with a
  no-close-X tab strip and `pane.stack.cycle/add/remove` actions —
  DEFERRED until the vendored pane_group refactor lands
:::

## Implementation

`vendor/zed/crates/workspace/src/codon_bridge.rs` exposes the snapshot
types and the `apply_layout` / `capture_layout` helpers.
`Workspace::replace_center_with_snapshot` lives in `workspace.rs` next
to `serialize_workspace_internal`.
