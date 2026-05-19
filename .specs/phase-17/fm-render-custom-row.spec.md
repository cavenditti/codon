---
id: TASK:phase-17/fm-render-custom-row
type: task
status: draft
version: 0.0.1
summary: >
  Replace the nested-Div row in the file manager with a single
  custom `gpui::Element` impl that paints background, icon glyph,
  name + meta glyph runs, and optional git-status decoration
  directly into the scene — no `Interactivity`, no `with_text_style`,
  no `with_image_cache`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-render#c-custom-row-element
aspects: [custom-element, paint, no-interactivity, glyph-runs]
---

# Custom row Element

## What changes

Create `crates/file-manager/src/render/row.rs` exporting a
`FmRowElement` that implements `gpui::Element` directly (not via
`Div`). Move the row-painting logic out of
`crates/file-manager/src/view.rs::render_entry_row` into the new
Element.

The Element's contract:

- **`request_layout`** returns a fixed `Size` derived from the
  column's row-height and width — no taffy descent. Layout is a
  pure function of inputs.
- **`paint`** does, in order:
  1. One `PaintQuad` for the row background (alternating zebra
     stripe, selection, or mark colour).
  2. One glyph run for the icon, sourced from the cached
     `icon_path` and the platform text system (or a single
     pre-rasterised SVG path emitted via `gpui::svg`'s low-level
     path API).
  3. One glyph run for the `name` SharedString.
  4. One glyph run per non-empty `meta[i]` SharedString
     (size / mtime / count / git / mode).
  5. One optional status decoration (a coloured square at the
     trailing edge if `git_status.is_some()`).
- Hit testing is captured at the FM view level via the codon TOML
  keymap; the row Element does not register click / hover / action
  listeners. The view's existing `on_action(cx.listener(...))`
  handlers continue to dispatch `FileManager::on_*` actions.

Inputs to `FmRowElement::new`:

```rust
pub(crate) struct FmRowElement {
    entry: Arc<DirEntry>,             // already has cached labels + icon
    row_index: usize,
    state: RowDisplayState,           // selection, mark, focus, zebra
    metrics: RowMetrics,              // x offsets per column-of-meta, row height
    theme: Arc<RowTheme>,             // resolved colours per state
}
```

Construction is cheap (just `Arc` clones); the Element owns no
event handlers and no closures.

## Why this clause

Per the phase-17 profile, `Div::Interactivity::paint` runs ~6
nested closures per visible row plus `with_text_style`,
`with_image_cache`, `with_optional_element_state` calls. None of
that overhead is used by the FM, but it shows up in every paint
sample. A custom Element collapses ~30 GPUI calls per row into ~5
direct scene insertions. The expected per-row reduction is
≈ 6× and per-frame ≈ 2.5× before any caching.

## Verification

- `cargo test  -p file-manager` continues to pass.
- A new unit test `fm_row_element_paint_emits_expected_scene`
  constructs a row with synthetic state and asserts that the
  scene receives exactly the expected sequence of quad / glyph
  insertions (no taffy nodes, no Div Interactivity slots).
- The render-trace harness (task
  `phase-17/fm-render-frame-budget`) reports a measurable drop
  in per-row paint cost.

## Done when

- `FmRowElement` exists and is used by the FM view behind the
  `[file_manager] custom_render = true` flag.
- The legacy `render_entry_row` path remains compilable and
  reachable when `custom_render = false`.
- `cargo clippy -p file-manager` is clean.
- `spec lint` is at zero errors.
