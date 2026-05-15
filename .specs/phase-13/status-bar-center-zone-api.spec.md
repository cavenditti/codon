---
id: TASK:phase-13/status-bar-center-zone-api
type: task
status: accepted
version: 0.0.1
summary: >
  Grow vendored Zed's StatusBar with a center_items vec, an
  add_center_item constructor, and a three-cell render path.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/status-bar#c-vendored-zone-api
  - REQ:codon/status-bar#c-three-zones
aspects: [vendored-vec-and-constructor, three-cell-render]
---

# Vendored Zed: add a real centre zone to StatusBar

## What changes

`vendor/zed/crates/workspace/src/status_bar.rs` currently models the
bar as `left_items: Vec<...>` + `right_items: Vec<...>` rendered as
a two-child `justify_between` row. To satisfy
[REQ:codon/status-bar#c-three-zones](spec:REQ:codon/status-bar#c-three-zones)
the struct needs a third vec and a third render cell.

Concrete shape:

```rust
pub struct StatusBar {
    left_items:   Vec<Box<dyn StatusItemViewHandle>>,
    center_items: Vec<Box<dyn StatusItemViewHandle>>,
    right_items:  Vec<Box<dyn StatusItemViewHandle>>,
    // …existing fields…
}

impl StatusBar {
    pub fn add_center_item<T>(&mut self, item: Entity<T>, window: &mut Window, cx: &mut Context<Self>)
    where T: 'static + StatusItemView { … }

    fn render_center_tools(&self, cx: &mut Context<Self>) -> impl IntoElement { … }
}
```

The `Render` impl swaps `justify_between` (two children) for an
explicit three-cell flex:

- left cell: `min_w_0`, `overflow_x_hidden`, `flex_shrink` (highest
  priority — these items lose pixels last);
- centre cell: `flex_1`, `min_w_0`, `overflow_x_hidden`,
  `justify_center`;
- right cell: `flex_shrink_0`, `overflow_x_hidden`.

Existing traversal helpers — `item_of_type`, `position_of_item`,
`insert_item_after`, `remove_item_at`, `update_active_pane_item` —
MUST walk all three vecs. `position_of_item` numbers items
left → centre → right; `insert_item_after` / `remove_item_at`
translate the absolute index into the right vec.

## Approach

1. Edit `vendor/zed/crates/workspace/src/status_bar.rs`:
   - Add `center_items` field with `Default::default()` in `new`.
   - Implement `add_center_item` mirroring `add_left_item`
     (call `set_active_pane_item` before pushing, then `cx.notify()`).
   - Implement `render_center_tools` mirroring `render_left_tools`
     but without the sidebar-toggle path.
   - Update `render` to lay out three cells.
   - Update `item_of_type` to chain all three vecs.
   - Update `position_of_item` to number left → centre → right.
   - Update `insert_item_after` / `remove_item_at` to handle the
     three-segment index space.
   - Update `update_active_pane_item` to iterate all three vecs.
2. Commit on the `codon` submodule branch with `Spec-Ref:` trailers.
3. Bump the submodule pointer in the outer repo.

## Non-goals

- No new Zed setting for "show / hide zones". Codon's render is
  unconditional.
- No removal of `add_left_item` / `add_right_item`. Both stay so
  any upstream re-merge keeps compiling.
- No public surface for reordering items within a zone — codon
  registers them in the desired order at startup.

## Files touched

- `vendor/zed/crates/workspace/src/status_bar.rs` — additive
  changes outlined above.

## Verification

- `cargo check -p workspace` (from `vendor/zed`) passes.
- `vendor/zed/script/clippy` reports no new warnings.
- A throwaway codon-side smoke test that calls `add_center_item`
  with the existing `ActiveFileName` item produces a centred
  segment between left and right zones at runtime.
