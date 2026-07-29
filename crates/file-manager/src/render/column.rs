//! Custom `gpui::Element` for an entire FM column.
//!
//! Replaces the per-column `uniform_list` body with a single Element
//! that owns:
//!
//! - virtualization (compute `first_visible_idx` / `last_visible_idx`
//!   from a single `f32` scroll offset + the configured row height);
//! - inline row painting (build a `FmRowElement` per visible row
//!   inside `prepaint` and forward `paint` directly to it);
//! - scrollbar (two `PaintQuad`s — track + thumb — sized to the
//!   visible-fraction-of-total);
//! - a dirty-row hint surface (`mark_rows_dirty`) consumed by
//!   `fm-render-dirty-rect`.
//!
//! Scrollbar interactivity is intentionally absent — codon is
//! keyboard-first, j/k drive the selection-tracking scroll at the FM
//! view level, and the scrollbar is purely an indicator.
//!
//! The column does not register click / hover / action listeners;
//! hit-testing stays at the FM view via the codon TOML keymap.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, SharedString, Style, Window, fill, point, px, size,
};

use crate::file_manager::DirEntry;
use crate::prefs::LineMode;
use crate::render::row::{FmRowElement, RowDisplayState, RowMetrics, RowTheme};
use crate::render::row_glyph_cache::{CachedRow, RowGlyphCache, RowGlyphKey};
use crate::render::shaped_line_cache::ShapedLineCache;
use crate::render::trace::COUNTERS;

/// Which of the three FM columns this Element renders. Only used to
/// tag log lines + future per-column tuning; layout is identical
/// across kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum ColumnKind {
    Parent,
    Current,
    Preview,
}

/// Per-column theme bundle. Re-uses the row theme so the column +
/// row Elements share resolved colours without re-walking
/// `cx.theme()` per row.
#[derive(Clone, Debug)]
pub(crate) struct ColumnTheme {
    pub row: Arc<RowTheme>,
    pub scrollbar_track: Hsla,
    pub scrollbar_thumb: Hsla,
}

/// Construction inputs for the custom column.
pub(crate) struct FmColumnElement {
    #[allow(dead_code)]
    pub column_kind: ColumnKind,
    pub entries: Arc<[Arc<DirEntry>]>,
    pub selection: Option<usize>,
    pub marks: Arc<std::collections::HashSet<usize>>,
    pub theme: Arc<ColumnTheme>,
    pub row_metrics: RowMetrics,
    pub line_mode: LineMode,
    /// Logical scroll offset in pixels. Owned by the column for now;
    /// future work threads this back through the FM's
    /// `scroll_handle` so j/k keep working as today. While the flag
    /// is in alpha, the column tracks its own offset and the
    /// selection-clamping logic in the view keeps the cursor on
    /// screen.
    pub scroll_offset: Rc<RefCell<f32>>,
    /// Shaped-line cache shared across every visible row in this
    /// column for the lifetime of the paint.
    pub shaped_line_cache: Rc<RefCell<ShapedLineCache>>,
    /// Row-glyph cache — sits one layer above the shaped-line cache
    /// and stores the fully-resolved row payload (bg colour + shaped
    /// name + shaped meta) keyed on `(path, display_state,
    /// line_mode)`. On selection move, only the two affected rows
    /// (previously- and newly-selected) are cache misses.
    pub row_glyph_cache: Rc<RefCell<RowGlyphCache>>,
    /// Optional dirty-row hint: when populated by
    /// `mark_rows_dirty`, paint only the listed rows + the
    /// previously-selected row, leaving the rest as-is. Empty means
    /// "full repaint" — the default for the first frame.
    pub dirty_rows: Rc<RefCell<DirtyRows>>,
    /// Whether the column body is dimmed (parent + preview columns).
    pub dimmed: bool,
}

/// Hint set populated by `mark_rows_dirty` / `mark_all_dirty`.
///
/// `indices` are the row positions whose cached payload must be
/// rebuilt on the next paint — typically the previous + new
/// selection on a `j` / `k` move. `all` is the "everything is
/// stale" override (e.g. theme change, directory rotation).
///
/// Investigation note for `fm-render-dirty-rect`: GPUI's
/// `Scene::replay` (`vendor/zed/crates/gpui/src/scene.rs:127`) can
/// only replay an entire range of the previous scene's
/// `paint_operations`; there's no public API to combine a partial
/// replay with new contributions inside a custom `Element::paint`
/// (Path A in the spec). The reachable shape today is Path B:
/// emit a full scene each frame, but make the row-glyph cache the
/// authority for the per-row payload so non-dirty rows are
/// O(1) lookups while dirty rows pay the full prepaint cost.
#[derive(Default, Debug)]
pub(crate) struct DirtyRows {
    pub indices: Vec<usize>,
    pub all: bool,
}

impl FmColumnElement {
    /// Hint that the listed rows' cached payloads are stale and
    /// must be rebuilt on the next paint. Selection changes do not
    /// need this because the complete selection state is already in
    /// `RowGlyphKey`; this remains the invalidation surface for row
    /// inputs that are not represented by that key.
    #[allow(dead_code)] // retained as the explicit invalidation surface for non-keyed row changes
    pub fn mark_rows_dirty(dirty: &Rc<RefCell<DirtyRows>>, indices: &[usize]) {
        let mut state = dirty.borrow_mut();
        for &i in indices {
            if !state.indices.contains(&i) {
                state.indices.push(i);
            }
        }
    }

    /// Hint that every row's cached payload is stale — used on
    /// theme change, directory rotation, and mark-set changes
    /// (since the mark-set flips bg colours across many rows at
    /// once, an all-clear is cheaper than enumerating).
    pub fn mark_all_dirty(dirty: &Rc<RefCell<DirtyRows>>) {
        let mut state = dirty.borrow_mut();
        state.all = true;
        state.indices.clear();
    }

    fn first_last_visible(&self, bounds: Bounds<Pixels>) -> (usize, usize) {
        let row_h = f32::from(self.row_metrics.row_height).max(1.0);
        let h = f32::from(bounds.size.height).max(0.0);
        let viewport_rows = ((h / row_h).ceil() as usize).max(1);
        // If the selection-driven offset wasn't initialised by the
        // view (`scroll_offset.borrow() == 0.0` and a selection
        // exists below the viewport), nudge it so the selected row
        // is in view. Keeps j/k working without requiring the FM
        // view to keep `scroll_offset` in sync every frame.
        let max_idx = self.entries.len();
        let mut offset = *self.scroll_offset.borrow();
        if let Some(sel) = self.selection {
            let sel_top = (sel as f32) * row_h;
            let sel_bottom = sel_top + row_h;
            if sel_top < offset {
                offset = sel_top;
            } else if sel_bottom > offset + h {
                offset = (sel_bottom - h).max(0.0);
            }
        }
        let max_offset = ((max_idx as f32) * row_h - h).max(0.0);
        offset = offset.clamp(0.0, max_offset);
        *self.scroll_offset.borrow_mut() = offset;

        let first = (offset / row_h).floor().max(0.0) as usize;
        let last = first.saturating_add(viewport_rows + 1);
        (first.min(max_idx), last.min(max_idx))
    }
}

impl IntoElement for FmColumnElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// State carried from `prepaint` to `paint`. Holds the constructed
/// row Elements for the visible window so `paint` can drive them
/// inline.
pub(crate) struct FmColumnPrepaint {
    visible_rows: Vec<(
        Bounds<Pixels>,
        FmRowElement,
        FmRowPrepaintLite,
        Arc<CachedRow>,
    )>,
    track_bounds: Option<Bounds<Pixels>>,
    thumb_bounds: Option<Bounds<Pixels>>,
}

/// Hand-rolled stand-in for `FmRowElement::PrepaintState` — we hold
/// the same shaped lines but drive paint via a per-row scratch
/// rather than recursing through `Element::paint`, which would
/// require feeding GPUI a `LayoutId` for each row.
pub(crate) struct FmRowPrepaintLite {
    #[allow(dead_code)]
    pub state: RowDisplayState,
    #[allow(dead_code)]
    pub meta_text: Option<SharedString>,
}

impl Element for FmColumnElement {
    type RequestLayoutState = ();
    type PrepaintState = FmColumnPrepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = gpui::relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let trace_started = std::time::Instant::now();
        // Consume the dirty-rows hint up front so the next frame
        // starts clean unless the view re-marks. `all` clears the
        // row-glyph cache wholesale; individual dirty indices are
        // collected into a `HashSet` and used to bypass the cache
        // per-row below.
        let (dirty_all, dirty_set) = {
            let mut state = self.dirty_rows.borrow_mut();
            let all = state.all;
            state.all = false;
            let set: std::collections::HashSet<usize> =
                std::mem::take(&mut state.indices).into_iter().collect();
            (all, set)
        };
        if dirty_all {
            self.row_glyph_cache.borrow_mut().clear();
        }

        let (first, last) = self.first_last_visible(bounds);
        let row_h = self.row_metrics.row_height;
        let mut visible_rows = Vec::with_capacity(last.saturating_sub(first));
        let offset_y = px(*self.scroll_offset.borrow());

        for i in first..last {
            let row_origin = point(
                bounds.origin.x,
                bounds.origin.y + row_h * (i as f32) - offset_y,
            );
            let row_bounds = Bounds {
                origin: row_origin,
                size: size(bounds.size.width, row_h),
            };
            let entry = self.entries[i].clone();
            let is_selected = self.selection == Some(i);
            let is_marked = self.marks.contains(&i);
            let state = RowDisplayState {
                is_selected,
                is_marked,
                is_focused_row: is_selected && !self.dimmed,
                zebra_stripe: false,
            };
            let meta_text = entry.labels.meta[self.line_mode.idx()].clone();
            let mut row = FmRowElement {
                entry: entry.clone(),
                row_index: i,
                state,
                metrics: self.row_metrics,
                theme: self.theme.row.clone(),
                meta_text: meta_text.clone(),
                shaped_line_cache: self.shaped_line_cache.clone(),
            };

            // Row-glyph cache lookup. The cached payload composes
            // the shaped name + meta lines + resolved background;
            // a hit skips all per-row state-derivation work.
            let icon_path_key = entry.icon_path.as_ref().and_then(|o| o.clone());
            let key = RowGlyphKey {
                path: entry.path.clone(),
                line_mode: self.line_mode,
                state,
                name_text: entry.labels.name.clone(),
                meta_text: meta_text.clone(),
                git_status: entry.git_status,
                is_dir: entry.is_dir,
                is_hidden: entry.is_hidden,
                is_symlink: entry.is_symlink,
                mode: entry.mode,
                icon_path: icon_path_key,
            };
            let force_rebuild = dirty_set.contains(&i);
            let cached = if force_rebuild {
                None
            } else {
                let mut cache = self.row_glyph_cache.borrow_mut();
                cache.get(&key)
            };
            let payload = match cached {
                Some(c) => c,
                None => {
                    let inline = row.prepaint_inline(row_bounds, window, _cx);
                    let c = Arc::new(CachedRow {
                        background: row.background_color_inline(),
                        name_line: inline.name_line.clone(),
                        meta_line: inline.meta_line.clone(),
                    });
                    self.row_glyph_cache.borrow_mut().insert(key, c.clone());
                    c
                }
            };
            visible_rows.push((
                row_bounds,
                row,
                FmRowPrepaintLite { state, meta_text },
                payload,
            ));
        }

        // Scrollbar: track always painted, thumb sized to the visible
        // fraction. Hidden when the entry list fits in the viewport.
        let total_rows = self.entries.len();
        let (track_bounds, thumb_bounds) = if total_rows == 0 {
            (None, None)
        } else {
            let content_h = row_h * (total_rows as f32);
            let viewport_h = bounds.size.height;
            if content_h <= viewport_h {
                (None, None)
            } else {
                let track_w = px(2.0);
                let track_x = bounds.origin.x + bounds.size.width - track_w - px(1.0);
                let track = Bounds {
                    origin: point(track_x, bounds.origin.y),
                    size: size(track_w, viewport_h),
                };
                let visible_frac = (f32::from(viewport_h) / f32::from(content_h)).clamp(0.0, 1.0);
                let thumb_h = (viewport_h * visible_frac).max(px(8.0));
                let max_offset = (f32::from(content_h) - f32::from(viewport_h)).max(0.0);
                let scroll_frac = if max_offset > 0.0 {
                    (f32::from(offset_y) / max_offset).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let thumb_y = bounds.origin.y + (viewport_h - thumb_h) * scroll_frac;
                let thumb = Bounds {
                    origin: point(track_x, thumb_y),
                    size: size(track_w, thumb_h),
                };
                (Some(track), Some(thumb))
            }
        };

        let result = FmColumnPrepaint {
            visible_rows,
            track_bounds,
            thumb_bounds,
        };
        COUNTERS.add_prepaint_duration(trace_started.elapsed());
        result
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let trace_started = std::time::Instant::now();
        let visible_rows = std::mem::take(&mut prepaint.visible_rows);
        let mut painted = 0u64;
        // Path B (per the spec's investigation note): GPUI's
        // `Scene::replay` can only replay an entire range of a
        // previous scene's `paint_operations`, with no public API
        // to mix partial replay with new contributions inside a
        // custom `Element::paint`. So every visible row paints
        // each frame — but the row-glyph cache makes non-dirty
        // rows an O(1) lookup. Dirty rows pay the prepaint cost
        // (handled upstream in `prepaint`); paint itself is
        // uniform.
        for (row_bounds, mut row, lite, payload) in visible_rows {
            let mut prepaint_row = RowInlinePrepaint {
                name_line: payload.name_line.clone(),
                meta_line: payload.meta_line.clone(),
            };
            row.paint_inline(row_bounds, &mut prepaint_row, lite, window, cx);
            painted += 1;
        }
        COUNTERS.add_rows_repainted(painted);

        // Scrollbar — track first, thumb second so the thumb sits on
        // top. Both clipped to the column bounds by GPUI.
        if let Some(track) = prepaint.track_bounds.take() {
            window.paint_quad(fill(track, self.theme.scrollbar_track));
        }
        if let Some(thumb) = prepaint.thumb_bounds.take() {
            window.paint_quad(fill(thumb, self.theme.scrollbar_thumb));
        }
        COUNTERS.add_paint_duration(trace_started.elapsed());
    }
}

impl FmRowElement {
    /// Inline alternative to `Element::prepaint` — returns just the
    /// shape state without going through Taffy / `request_layout`.
    /// Used by `FmColumnElement::paint` so the column drives the
    /// row's paint directly.
    pub(crate) fn prepaint_inline(
        &mut self,
        _bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut App,
    ) -> RowInlinePrepaint {
        let text_system = window.text_system().clone();
        let font_id = text_system.resolve_font(&self.theme.font);
        let font_size = self.metrics.font_size;
        let mut cache = self.shaped_line_cache.borrow_mut();

        let name_text = self.entry.labels.name.clone();
        let name_run = self.name_run_inline(&name_text);
        let name_line = if name_text.is_empty() {
            None
        } else {
            Some(cache.get_or_shape(&name_text, font_id, font_size, &text_system, &name_run))
        };

        let meta_line = self.meta_text.as_ref().map(|text| {
            let run = self.meta_run_inline(text);
            cache.get_or_shape(text, font_id, font_size, &text_system, &run)
        });

        RowInlinePrepaint {
            name_line,
            meta_line,
        }
    }

    fn name_run_inline(&self, text: &SharedString) -> gpui::TextRun {
        let mut font = self.theme.font.clone();
        if self.state.is_selected {
            font.weight = gpui::FontWeight::BOLD;
        }
        font.style = gpui::FontStyle::Normal;
        gpui::TextRun {
            len: text.len(),
            font,
            color: if self.state.is_marked {
                self.theme.accent_stripe
            } else {
                self.theme.text_default
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    fn meta_run_inline(&self, text: &SharedString) -> gpui::TextRun {
        gpui::TextRun {
            len: text.len(),
            font: self.theme.font.clone(),
            color: self.theme.text_muted,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    /// Inline alternative to `Element::paint`. Driven by the column
    /// Element so we don't pay for per-row taffy / GPUI element
    /// instantiation.
    pub(crate) fn paint_inline(
        &mut self,
        bounds: Bounds<Pixels>,
        prepaint: &mut RowInlinePrepaint,
        _lite: FmRowPrepaintLite,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(bg) = self.background_color_inline() {
            window.paint_quad(fill(bounds, bg));
        }

        if self.state.is_marked {
            let stripe_bounds = Bounds {
                origin: bounds.origin,
                size: size(px(2.0), self.metrics.row_height),
            };
            window.paint_quad(fill(stripe_bounds, self.theme.accent_stripe));
        }

        let mut pen_x = bounds.origin.x + self.metrics.left_pad + self.metrics.stripe_slot_width;
        pen_x += self.metrics.git_glyph_width + self.metrics.gap;

        let icon_size = self.metrics.icon_width;
        let icon_top_offset = ((self.metrics.row_height - icon_size) / 2.0).max(px(0.0));
        let icon_bounds = Bounds {
            origin: point(pen_x, bounds.origin.y + icon_top_offset),
            size: size(icon_size, icon_size),
        };
        if let Some(Some(icon_path)) = &self.entry.icon_path {
            if let Err(err) = window.paint_svg(
                icon_bounds,
                icon_path.clone(),
                None,
                gpui::TransformationMatrix::default(),
                self.theme.text_muted,
                cx,
            ) {
                log::debug!("fm row: paint_svg for {:?} failed: {err}", self.entry.path);
            }
        }
        pen_x += icon_size + self.metrics.gap;

        let text_baseline_y =
            bounds.origin.y + (self.metrics.row_height - self.metrics.font_size) / 2.0;
        if let Some(name_line) = prepaint.name_line.take() {
            let line = (*name_line).clone();
            if let Err(err) = line.paint(
                Point::new(pen_x, text_baseline_y),
                self.metrics.row_height,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            ) {
                log::debug!("fm row: name line paint failed: {err}");
            }
        }

        if let Some(meta_line) = prepaint.meta_line.take() {
            let meta_x = bounds.origin.x + bounds.size.width
                - self.metrics.right_pad
                - self.metrics.meta_width;
            let line = (*meta_line).clone();
            if let Err(err) = line.paint(
                Point::new(meta_x, text_baseline_y),
                self.metrics.row_height,
                gpui::TextAlign::Left,
                Some(self.metrics.meta_width),
                window,
                cx,
            ) {
                log::debug!("fm row: meta line paint failed: {err}");
            }
        }
    }

    pub(crate) fn background_color_inline(&self) -> Option<Hsla> {
        if self.state.is_selected {
            Some(self.theme.bg_selected)
        } else if self.state.is_marked {
            Some(self.theme.bg_marked)
        } else if self.state.zebra_stripe {
            Some(self.theme.bg_zebra)
        } else {
            None
        }
    }
}

/// Inline prepaint state used by `FmColumnElement` so row painting
/// can be driven directly from the column's `paint` step (no Taffy
/// descent, no per-row `LayoutId`).
pub(crate) struct RowInlinePrepaint {
    pub name_line: Option<Arc<gpui::ShapedLine>>,
    pub meta_line: Option<Arc<gpui::ShapedLine>>,
}
