---
id: TASK:phase-17/fm-render-dirty-rect
type: task
status: draft
version: 0.0.1
summary: >
  On selection movement, repaint only the two affected rows
  (previously-selected and newly-selected) if GPUI's
  `Scene::replay` supports incremental scene composition;
  otherwise emit a full scene whose row contributions come from
  the row-glyph cache. Expose `mark_rows_dirty(&[usize])` on the
  column Element.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-render#c-dirty-rect-repaint
aspects: [partial-repaint, scene-replay, dirty-set]
---

# Dirty-rect repaint

## What changes

The behaviour depends on what GPUI exposes:

**Path A — GPUI supports partial scene composition.** From the
samply trace we know `gpui::scene::Scene::replay` already
exists. If `Window` can compose a scene from a previously
replayed slice + new contributions, the FM column emits only
the dirty rows. Investigation step: read
`vendor/zed/crates/gpui/src/scene.rs` and document whether
replay can be combined with new contributions in a single
frame. If yes — wire it.

**Path B — GPUI does full-frame composition only.** The
fallback (and the more likely answer given GPUI's current
design) is to keep emitting a full scene, but make sure every
row whose `RowGlyphKey` is unchanged is served from the
row-glyph cache without re-positioning. Path B yields most of
the perceived speedup once
`phase-17/fm-render-row-glyph-cache` lands.

In either path, expose:

```rust
impl FmColumnElement {
    pub fn mark_rows_dirty(&self, rows: &[usize]) { ... }
    pub fn mark_all_dirty(&self) { ... }
}
```

The FM view calls `mark_rows_dirty(&[prev_selection, new_selection])`
on selection change, `mark_all_dirty()` on directory change /
theme change / mark-set change.

The "dirty set" lives next to the row-glyph cache: a
`HashSet<usize>` of row indices that must be rebuilt on next
paint. Non-dirty rows take the cached path
unconditionally.

## Why this clause

Selection movement is the highest-frequency repaint event in
the FM. Today every keystroke triggers a full prepaint walk
over visible rows. With Path B + row-glyph cache, the visible
work shrinks to "rebuild 2 rows, emit cached glyph vec for the
rest". With Path A, the scene itself only carries the 2-row
delta. Either way, we expect ≤ 1 ms / frame for selection
movement in steady state.

## Verification

- A one-page **investigation note** committed alongside the
  task documenting whether GPUI supports Path A (and if so
  how), or confirming Path B is the chosen route. (Stored at
  `crates/file-manager/docs/dirty-rect-investigation.md` if
  needed — the doc is a verification artifact, not a feature
  doc per CLAUDE.md's "don't write docs files unless
  requested" rule; this one is explicitly requested by this
  task.)
- New test `dirty_rect_selection_move_rebuilds_two_rows`
  asserts that after `mark_rows_dirty(&[3, 5])` only those
  two rows hit `build_row_glyphs`; rows 0..3, 4, 6..N take the
  cached fast path.
- Render-trace harness reports per-keystroke selection move
  at p95 ≤ 1 ms (excluding cache misses on first ever paint).

## Done when

- `mark_rows_dirty` is wired and exercised from the FM view.
- The chosen path (A or B) is documented in the investigation
  note.
- `spec lint` is at zero errors.
