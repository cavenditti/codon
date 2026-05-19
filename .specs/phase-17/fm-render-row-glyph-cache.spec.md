---
id: TASK:phase-17/fm-render-row-glyph-cache
type: task
status: draft
version: 0.0.1
summary: >
  Cache the entire row's pre-positioned `Vec<PaintGlyph>` keyed on
  `(path, display_state)`. On scroll, translate the cached glyphs
  by `(0, dy)` and emit untouched. On selection move, only the
  background quad of the two affected rows is recomputed.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-render#c-row-glyph-cache
aspects: [paint-cache, translation-only, scroll-reuse]
---

# Row-glyph cache

## What changes

Extend `crates/file-manager/src/render/row.rs` with a per-column
glyph cache:

```rust
#[derive(Hash, Eq, PartialEq, Clone)]
struct RowGlyphKey {
    path: PathBuf,
    line_mode: LineMode,
    git_decoration: Option<GitStatusKind>,
    mark_state: MarkState,                 // unmarked | marked
    selection_state: SelectionState,       // unselected | selected | cursor
}

struct CachedRow {
    background: PaintQuad,
    glyphs: SmallVec<[PaintGlyph; 16]>,
    status_decoration: Option<PaintQuad>,
    /// Position glyphs were laid out at; on paint we translate by
    /// `(target_origin - origin) ` rather than re-laying.
    origin: Point<Pixels>,
}

pub(crate) struct RowGlyphCache {
    inner: LruCache<RowGlyphKey, Arc<CachedRow>>,
}
```

`FmRowElement::paint` becomes:

```rust
let key = RowGlyphKey::derive(&self.entry, &self.state, &self.metrics);
let row = self.glyph_cache.borrow_mut().get_or_build(&key, || {
    build_row_glyphs(&self.entry, &self.state, &self.metrics, &self.theme,
                     &mut self.shaped_line_cache.borrow_mut(), text_system)
});
emit_translated(scene, &row, target_origin);
```

`emit_translated` does *not* call back into the text system; it
walks the cached glyph vec, offsets each `PaintGlyph.origin` by
`target_origin - cached.origin`, and pushes into the scene.

Critical invariant: selection movement does **not** invalidate
the glyph cache of the non-selected rows. The
`selection_state` field is only part of the key for the two rows
that toggle between selected and unselected. The column Element
maintains the two changed indices explicitly and rebuilds those
two entries; everyone else reuses.

## Why this clause

The shaped-line cache (`phase-17/fm-render-shaped-line-cache`)
removes the cost of re-shaping the same SharedString, but each
frame still pays the cost of positioning glyphs into a row layout
(icon offset, name x, meta column offsets) and constructing
`PaintGlyph` records. For an unchanged row this is pure
recomputation. Caching the full positioned vec collapses paint
to a translate-and-emit step. Expected saving: 3–4 ms on a
30-row redraw, dwarfing the shaped-line cache's saving on top of
which it sits.

## Verification

- New test `row_glyph_cache_scroll_reuse` paints a row, scrolls,
  paints again at a new y; asserts that the cached entry was
  reused (cache hit counter) and that the second paint produced
  a scene whose glyph positions are correctly translated.
- New test `row_glyph_cache_selection_partial_invalidation`
  moves the selection from row 5 to row 7 and asserts that only
  rows 5 and 7 are rebuilt (miss counter == 2; hit counter ==
  visible_rows - 2).
- Render-trace harness reports per-frame row-glyph cache
  hit-rate ≥ 95% during j/k navigation, ≥ 80% during
  page-scroll, drops to ~0% on `cd` into a new directory
  (expected — different paths).

## Done when

- `RowGlyphCache` is wired into `FmColumnElement`.
- Selection-change repaints only rebuild affected rows.
- Theme change invalidates the entire glyph cache (it composes
  with `ShapedLineCache::invalidate_for_font`).
- `spec lint` is at zero errors.
