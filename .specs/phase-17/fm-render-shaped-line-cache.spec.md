---
id: TASK:phase-17/fm-render-shaped-line-cache
type: task
status: draft
version: 0.0.1
summary: >
  Add an FM-scoped LRU cache over `ShapedLine` results keyed on
  `(font_id, font_size, string)`, so the same name / meta string
  is shaped at most once per visible-set lifetime instead of every
  frame.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-render#c-shaped-line-cache
aspects: [text-shaping, lru, theme-invalidation]
---

# FM-scoped shaped-line LRU

## What changes

Create `crates/file-manager/src/render/shaped_line_cache.rs`
exporting:

```rust
pub(crate) struct ShapedLineCache {
    inner: LruCache<ShapedLineKey, Arc<ShapedLine>>,
    font_id: FontId,
    font_size: Pixels,
    capacity: usize,
}

#[derive(Hash, Eq, PartialEq)]
struct ShapedLineKey {
    font_id: FontId,
    font_size: u32,        // px * 100, integer-keyed
    text: SharedString,
}
```

API:

```rust
impl ShapedLineCache {
    pub fn new(capacity: usize) -> Self;
    pub fn get_or_shape(
        &mut self,
        text: &SharedString,
        font_id: FontId,
        font_size: Pixels,
        text_system: &WindowTextSystem,
    ) -> Arc<ShapedLine>;
    pub fn invalidate_for_font(&mut self, font_id: FontId, font_size: Pixels);
}
```

The custom row Element holds a `Rc<RefCell<ShapedLineCache>>`
borrowed from the parent column. Capacity defaults to
`4 × visible_rows × columns × meta_columns` (≈ 600 entries for
the typical layout) — enough to cover scroll headroom plus
preview lookahead.

Theme reactivity: the column subscribes to settings/theme
changes; on event, calls `invalidate_for_font` with the new
`(font_id, font_size)`. Entries with stale fonts are dropped on
next `get_or_shape` access.

## Why this clause

GPUI's `LineLayoutCache` (`gpui::text_system::line_layout`) is a
process-wide cache governed by `with_text_style` boundaries. From
the profile, `WindowTextSystem::shape_line` still appears on
every preview-change frame and during fast j/k scrolling.
An FM-scoped cache with a known-bounded keyset (visible names +
meta labels only) hits ~100% on steady-state navigation. The
per-frame text-shaping cost goes from "shape each visible row's
columns" to "lookup each visible row's columns" — at least
3 ms/frame saved on a 30-row, 4-meta-column layout.

## Verification

- New unit test `shaped_line_cache_lru_eviction` asserts that
  filling beyond capacity evicts least-recently-used entries
  and preserves the most-recently-accessed ones.
- New test `shaped_line_cache_font_invalidation` verifies that
  changing `(font_id, font_size)` discards all stale entries
  without leaking memory.
- Render-trace harness reports per-frame shape-cache hit-rate
  ≥ 90% during a 60-second navigation session over the
  reference 500-entry tree.

## Done when

- `ShapedLineCache` is instantiated once per `FmColumnElement`.
- `FmRowElement::paint` reads cached glyphs via the cache
  rather than calling the text system directly.
- `spec lint` is at zero errors.
