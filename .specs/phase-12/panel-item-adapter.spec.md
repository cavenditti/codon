---
id: TASK:phase-12/panel-item-adapter
type: task
status: accepted
version: 0.0.1
summary: >
  Build the generic PanelItemAdapter<P: Panel> that hosts any Zed
  Panel impl as a workspace::Item — the core mechanism Phase 12
  is built on.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-adapter
---

# Generic Panel → Item adapter

## What ships

A new crate `crates/codon-panes/` exposing:

```rust
pub struct PanelItemAdapter<P: workspace::Panel> {
    inner: gpui::Entity<P>,
    focus_handle: gpui::FocusHandle,
}

impl<P: workspace::Panel> workspace::Item for PanelItemAdapter<P> { … }
```

Forwards `Render`, `Focusable`, and `EventEmitter<PanelEvent>` to the
inner panel; sources `tab_content` / `tab_icon` from the panel's
`persistent_name` + `icon` + `icon_label`; routes `Item` focus to the
panel's `focus_handle`; calls `inner.set_active(true)` on tab activation
and `set_active(false)` on deactivation.

## Approach

1. New crate `codon-panes` added to the workspace `Cargo.toml`. Deps:
   `gpui`, `workspace`, `ui`, `codon-mode`. No deps on the individual
   panel crates (the adapter is generic over `P: Panel`).
2. Implement `Item` minimally — `tab_content`, `tab_icon`,
   `focus_handle`, `act_as_type`, `is_dirty` (always false for v1),
   `clone_on_split` (return `None` initially — panels are typically
   singletons). `telemetry_event_text` returns the `persistent_name`.
3. Forward `EventEmitter<PanelEvent>` so existing panel callers
   (e.g. agent's `ZoomIn` event) still observe state transitions; the
   adapter ignores `ZoomIn` / `ZoomOut` and emits a no-op for the
   pane (zoom is the workspace's job, not ours).
4. The adapter's constructor takes an `Entity<P>` (already built —
   each panel has its own `load`/`new` factory). It does not call the
   factory itself.
5. Unit test: instantiate the adapter over the existing `TestPanel`
   in [`vendor/zed/crates/workspace/src/dock.rs`](spec:src:vendor/zed/crates/workspace/src/dock.rs)
   and assert (a) focus reaches the panel, (b) `set_active` is
   invoked on tab activation, (c) `tab_content` renders the
   `persistent_name`.

## Non-goals

- No vendored Zed edits in this task (the `Panel` trait is consumed
  as-is). If `Workspace::open_item` needs a new public arm for
  inserting a pre-built `Box<dyn ItemHandle>`, that's a follow-up
  patch noted in `#c-adapter` of the REQ — not part of this task.
- No concrete panel wiring. The per-panel `Open*` actions land in
  the migration tasks; this task only ships the mechanism.

## Files touched

- `crates/codon-panes/` (new) — `Cargo.toml`, `src/lib.rs`,
  `src/adapter.rs`, `src/adapter_tests.rs`.
- Workspace `Cargo.toml` — add `codon-panes` as a member.
