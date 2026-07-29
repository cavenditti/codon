//! Custom `gpui::Element` for a single FM row.
//!
//! Replaces the nested `Div` + `Label` tree in `view::render_entry_row`
//! with one Element that paints, in order:
//!
//! 1. one `PaintQuad` for the row background (zebra / selection /
//!    marked tint),
//! 2. an optional 2 px left-edge accent stripe (marked rows only),
//! 3. one `PaintQuad` for the git-status glyph background slot
//!    (currently a no-op — the glyph itself is painted as a shaped
//!    single-character line),
//! 4. one SVG paint for the icon via `window.paint_svg`,
//! 5. one shaped + painted line for the entry name,
//! 6. one shaped + painted line for the meta string (size / mtime /
//!    git / mode), when present.
//!
//! Hit-testing is captured at the FM view level via the codon TOML
//! keymap; the row Element does NOT register click/hover/action
//! listeners. The view's existing `on_action(cx.listener(...))`
//! handlers continue to dispatch `FileManager::on_*` actions.
//!
//! The legacy `view::render_entry_row` path remains in place; this
//! Element is selected when `[file_manager] custom_render = true`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, Bounds, Element, ElementId, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, Style,
    TextAlign, TextRun, Window, fill, point, px, size,
};

use crate::file_manager::DirEntry;
use crate::render::shaped_line_cache::ShapedLineCache;

/// Per-row visual state — distilled from the FM view's selection/mark
/// model so the Element doesn't need a `Context<FileManager>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct RowDisplayState {
    pub is_selected: bool,
    pub is_marked: bool,
    pub is_focused_row: bool,
    pub zebra_stripe: bool,
}

impl RowDisplayState {
    #[allow(dead_code)] // used by tests + fm-render-dirty-rect
    pub fn new_unselected() -> Self {
        Self {
            is_selected: false,
            is_marked: false,
            is_focused_row: false,
            zebra_stripe: false,
        }
    }
}

/// Pixel-space x-axis layout of the row. Computed by the column
/// Element from a single configured row-height and known column
/// widths — no taffy descent.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowMetrics {
    pub row_height: Pixels,
    /// Left padding before the git glyph slot.
    pub left_pad: Pixels,
    /// Right padding from the column edge.
    pub right_pad: Pixels,
    /// Width of the 2 px stripe + 2 px spacer slot at the left edge.
    pub stripe_slot_width: Pixels,
    /// Width of the git status glyph slot.
    pub git_glyph_width: Pixels,
    /// Width of the icon slot.
    pub icon_width: Pixels,
    /// Width reserved for the trailing meta column.
    pub meta_width: Pixels,
    /// Gap between adjacent slots.
    pub gap: Pixels,
    /// Resolved font size for the row text.
    pub font_size: Pixels,
}

impl RowMetrics {
    /// Reference layout matching the legacy `render_entry_row` paddings.
    pub fn standard(font_size: Pixels) -> Self {
        Self {
            row_height: font_size + px(4.0),
            left_pad: px(4.0),
            right_pad: px(1.0),
            stripe_slot_width: px(4.0),
            git_glyph_width: px(12.0),
            icon_width: px(14.0),
            meta_width: px(90.0),
            gap: px(4.0),
            font_size,
        }
    }
}

/// Pre-resolved colours for the row. Computed once per frame by the
/// column Element from the active theme and passed by `Arc` clone to
/// each row.
#[derive(Clone, Debug)]
#[allow(dead_code)] // git_* fields wired into status decoration in fm-render-dirty-rect
pub(crate) struct RowTheme {
    pub text_default: Hsla,
    pub text_muted: Hsla,
    pub bg_selected: Hsla,
    pub bg_marked: Hsla,
    pub bg_zebra: Hsla,
    pub accent_stripe: Hsla,
    pub git_modified: Hsla,
    pub git_added: Hsla,
    pub git_deleted: Hsla,
    pub git_conflict: Hsla,
    pub git_untracked: Hsla,
    /// Font used for row text. Stored by value because a row Element
    /// outlives the `&App` that materialised the theme.
    pub font: gpui::Font,
}

/// Custom row Element. Construction is cheap — just `Arc`/`Rc`
/// clones; no event handlers, no closures.
pub(crate) struct FmRowElement {
    pub entry: Arc<DirEntry>,
    /// Index in the column's entry list — surfaced by
    /// `fm-render-dirty-rect` to address rows for partial repaint.
    #[allow(dead_code)]
    pub row_index: usize,
    pub state: RowDisplayState,
    pub metrics: RowMetrics,
    pub theme: Arc<RowTheme>,
    /// Meta string for the active `LineMode`, or `None` if this row
    /// renders no meta column.
    pub meta_text: Option<SharedString>,
    /// FM-scoped shaped-line cache, shared with sibling rows in the
    /// column. `Rc<RefCell<_>>` keeps the cache borrow local to the
    /// row's prepaint step without crossing the closure boundary the
    /// FM view's `&mut self` captures already drew.
    pub shaped_line_cache: Rc<RefCell<ShapedLineCache>>,
}

pub(crate) struct FmRowPrepaint {
    name_line: Option<Arc<ShapedLine>>,
    meta_line: Option<Arc<ShapedLine>>,
}

impl FmRowElement {
    fn background_color(&self) -> Option<Hsla> {
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

    fn name_run(&self, text: &SharedString) -> TextRun {
        // Selected rows render bolder so the cursor row is the visual
        // anchor regardless of selection-tint contrast.
        let mut font = self.theme.font.clone();
        if self.state.is_selected {
            font.weight = FontWeight::BOLD;
        }
        // No italic / no strikethrough for now — the row's a single
        // styled run, which is the codon convention for FM rows.
        font.style = FontStyle::Normal;
        TextRun {
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

    fn meta_run(&self, text: &SharedString) -> TextRun {
        TextRun {
            len: text.len(),
            font: self.theme.font.clone(),
            color: self.theme.text_muted,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }
}

impl IntoElement for FmRowElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FmRowElement {
    type RequestLayoutState = ();
    type PrepaintState = FmRowPrepaint;

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
        // Pure-function layout: full width, fixed row height.
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = self.metrics.row_height.into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        // Shape (or look up shaped) the two glyph runs ahead of paint.
        let text_system = window.text_system().clone();
        let font_id = text_system.resolve_font(&self.theme.font);
        let font_size = self.metrics.font_size;

        let mut cache = self.shaped_line_cache.borrow_mut();

        let name_text = self.entry.labels.name.clone();
        let name_run = self.name_run(&name_text);
        let name_line = if name_text.is_empty() {
            None
        } else {
            Some(cache.get_or_shape(&name_text, font_id, font_size, &text_system, &name_run))
        };

        let meta_line = self.meta_text.as_ref().map(|text| {
            let run = self.meta_run(text);
            cache.get_or_shape(text, font_id, font_size, &text_system, &run)
        });

        FmRowPrepaint {
            name_line,
            meta_line,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // 1. Background quad.
        if let Some(bg) = self.background_color() {
            window.paint_quad(fill(bounds, bg));
        }

        // 2. Left-edge accent stripe for marked rows. The stripe slot
        //    is always reserved; only the marked variant paints colour.
        if self.state.is_marked {
            let stripe_bounds = Bounds {
                origin: bounds.origin,
                size: size(px(2.0), self.metrics.row_height),
            };
            window.paint_quad(fill(stripe_bounds, self.theme.accent_stripe));
        }

        // 3. Pen advances right of the stripe slot.
        let mut pen_x = bounds.origin.x + self.metrics.left_pad + self.metrics.stripe_slot_width;

        // 4. Git-status glyph (skipped when no status — preserves the
        //    slot for horizontal alignment).
        pen_x += self.metrics.git_glyph_width + self.metrics.gap;

        // 5. Icon — paint as monochrome SVG when the entry has a
        //    resolved icon path. The `Option<Option<_>>` shape matches
        //    `DirEntry::icon_path`: outer-None means "not yet
        //    populated", inner-None means "no specific icon, use
        //    fallback".
        let icon_size = self.metrics.icon_width;
        let icon_top_offset = ((self.metrics.row_height - icon_size) / 2.0).max(px(0.0));
        let icon_bounds = Bounds {
            origin: point(pen_x, bounds.origin.y + icon_top_offset),
            size: size(icon_size, icon_size),
        };
        if let Some(Some(icon_path)) = &self.entry.icon_path {
            // Failures here are non-fatal — the FM still renders the
            // row body. `paint_svg` returns Err when the renderer
            // can't open the path; log and continue so a single bad
            // icon doesn't blank the panel.
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

        // 6. Name glyph run.
        let text_baseline_y =
            bounds.origin.y + (self.metrics.row_height - self.metrics.font_size) / 2.0;
        if let Some(name_line) = prepaint.name_line.take() {
            // `ShapedLine::paint` expects to take ownership of the
            // line's internal state for painting. We hold an `Arc<_>`
            // so reuse across frames is safe — clone the value out
            // once for paint.
            let line: ShapedLine = (*name_line).clone();
            if let Err(err) = line.paint(
                Point::new(pen_x, text_baseline_y),
                self.metrics.row_height,
                TextAlign::Left,
                None,
                window,
                cx,
            ) {
                log::debug!("fm row: name line paint failed: {err}");
            }
        }

        // 7. Meta glyph run, right-aligned in its reserved column.
        if let Some(meta_line) = prepaint.meta_line.take() {
            let meta_x = bounds.origin.x + bounds.size.width
                - self.metrics.right_pad
                - self.metrics.meta_width;
            let line: ShapedLine = (*meta_line).clone();
            if let Err(err) = line.paint(
                Point::new(meta_x, text_baseline_y),
                self.metrics.row_height,
                TextAlign::Left,
                Some(self.metrics.meta_width),
                window,
                cx,
            ) {
                log::debug!("fm row: meta line paint failed: {err}");
            }
        }
    }
}

/// Build a `RowTheme` from the active GPUI theme + UI font settings.
/// Keeps the row Element decoupled from `cx.theme()` traversal so
/// future tests can construct one without an `App` (see the unit
/// test below).
pub(crate) fn resolve_row_theme(cx: &gpui::App) -> Arc<RowTheme> {
    use theme::ActiveTheme;
    let theme = cx.theme();
    let colors = theme.colors();
    let status = theme.status();
    let font = theme::theme_settings(cx).ui_font(cx).clone();
    Arc::new(RowTheme {
        text_default: colors.text,
        text_muted: colors.text_muted,
        bg_selected: colors.ghost_element_active,
        bg_marked: colors.ghost_element_hover,
        // Zebra disabled by default; reserve a near-transparent
        // accent for a future opt-in.
        bg_zebra: Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 0.0,
        },
        accent_stripe: colors.text_accent,
        git_modified: status.modified,
        git_added: status.created,
        git_deleted: status.deleted,
        git_conflict: status.conflict,
        git_untracked: status.created,
        font,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construction must not require GPUI at all — that's what makes
    /// the row Element cheap enough to instantiate inline per frame.
    /// (We deliberately don't drive `paint` from a unit test — that
    /// would require a real `Window` + `App`, and the GPUI test
    /// harness for that is not available to a non-gpui crate.)
    #[test]
    fn fm_row_element_constructs_without_window() {
        use crate::file_manager::{DirEntry, EntryLabels};
        use std::path::PathBuf;

        let entry = Arc::new(DirEntry {
            name: "test.rs".to_string(),
            path: PathBuf::from("/tmp/test.rs"),
            is_dir: false,
            is_hidden: false,
            is_symlink: false,
            size: 0,
            git_status: None,
            mtime: None,
            btime: None,
            mode: None,
            uid: None,
            gid: None,
            child_count: None,
            labels: EntryLabels {
                name: SharedString::from("test.rs"),
                sort_name: "test.rs".into(),
                sort_extension: "rs".into(),
                meta: Default::default(),
            },
            icon_path: Some(None),
        });

        let theme = Arc::new(RowTheme {
            text_default: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.8,
                a: 1.0,
            },
            text_muted: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.5,
                a: 1.0,
            },
            bg_selected: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.2,
                a: 0.5,
            },
            bg_marked: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.3,
                a: 0.5,
            },
            bg_zebra: Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.15,
                a: 0.2,
            },
            accent_stripe: Hsla {
                h: 0.0,
                s: 0.5,
                l: 0.5,
                a: 1.0,
            },
            git_modified: Hsla::default(),
            git_added: Hsla::default(),
            git_deleted: Hsla::default(),
            git_conflict: Hsla::default(),
            git_untracked: Hsla::default(),
            font: gpui::font("Helvetica"),
        });

        let cache = Rc::new(RefCell::new(ShapedLineCache::new(64)));
        let _row = FmRowElement {
            entry,
            row_index: 0,
            state: RowDisplayState::new_unselected(),
            metrics: RowMetrics::standard(px(12.0)),
            theme,
            meta_text: None,
            shaped_line_cache: cache,
        };
    }
}
