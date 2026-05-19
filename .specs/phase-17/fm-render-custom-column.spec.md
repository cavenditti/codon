---
id: TASK:phase-17/fm-render-custom-column
type: task
status: draft
version: 0.0.1
summary: >
  Replace each FM column's `uniform_list` + outer `Div` with a
  single custom `gpui::Element` that owns its own virtualization,
  scrollbar, and inline row painting. Calls `FmRowElement::paint`
  directly without instantiating per-row Elements.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-render#c-custom-column-element
aspects: [custom-element, virtualization, scrollbar, inline-paint]
---

# Custom column Element

## What changes

Create `crates/file-manager/src/render/column.rs` exporting an
`FmColumnElement` that implements `gpui::Element` directly.
Replaces the per-column body in `view.rs` that today reads:

```rust
uniform_list(
    cx.entity().clone(),
    "fm-current",
    entry_count,
    move |this, range, _w, cx| { range.map(|i| render_entry_row(this, i, ...)) }
)
.size_full()
.flex_1()
```

with a single `FmColumnElement::new(state, column_kind, ...)`
construction. Behaviour:

- **`request_layout`** returns the column's available size; the
  Element owns layout entirely.
- **`prepaint`** computes
  `first_visible_idx = floor(scroll_y / row_height)` and
  `last_visible_idx = first_visible_idx + ceil(height / row_height)`.
  No iteration over the full entry list; no per-row Element
  instantiation.
- **`paint`** iterates the visible range and calls
  `FmRowElement::paint_inline(entry, row_index, state, cx)`
  directly for each visible row, then paints the scrollbar
  thumb / track as two `PaintQuad`s. (Scrollbar interactivity
  stays at the view level — codon's keymap drives scrolling, the
  drag affordance is intentionally absent per the keyboard-first
  rule.)
- Exposes `mark_rows_dirty(&[usize])` for the dirty-rect repaint
  task (`phase-17/fm-render-dirty-rect`) to plug into.

Construction signature:

```rust
pub(crate) struct FmColumnElement {
    column_kind: ColumnKind,                  // Parent | Current | Preview
    entries: Arc<[Arc<DirEntry>]>,
    scroll: ScrollState,
    selection: Option<usize>,
    marks: Arc<HashSet<PathBuf>>,
    theme: Arc<ColumnTheme>,
    row_metrics: RowMetrics,
}
```

The view's `Render` impl becomes a thin `h_flex()` of three
`FmColumnElement`s — but even the outer `h_flex` is a candidate
for replacement with a `FmRootElement` once we measure (out of
scope here; flagged as a follow-on in the phase summary).

## Why this clause

`uniform_list`'s `prepaint` walks the visible range, builds a
GPUI element per row, and runs each through the Div pipeline.
That is the bulk of the remaining cost after the row Element
lands — collapsing column + virtualization into one Element
brings the per-frame element count down by ~3 × (visible rows).
Per the profile, the column-level `Div` and its
`Interactivity::paint` are themselves on the hot path: removing
that wrapper cuts another fixed ~2 ms per redraw.

## Verification

- `cargo test  -p file-manager` continues to pass; existing
  scroll / virtualization tests are mirrored against the custom
  column path.
- New test `fm_column_element_visible_range` asserts that for
  a 1000-entry column at row_height 24px, scroll_y 1200px, and
  height 600px, only rows `[50..75]` are painted.
- `cargo clippy -p file-manager` is clean.

## Done when

- `FmColumnElement` exists and is used by `view.rs` behind
  `[file_manager] custom_render = true`.
- The previous `uniform_list` path is reachable when the flag
  is off (one-release-cycle escape hatch).
- The scrollbar renders, position tracks selection, no
  mouse-drag affordance is added.
- `spec lint` is at zero errors.
