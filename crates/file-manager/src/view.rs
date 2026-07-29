use git::status::FileStatus;
use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, ObjectFit, Render, SharedString,
    StyledImage, Window, div, img, prelude::*, px, relative, uniform_list,
};
use settings::Settings as _;
use theme::ActiveTheme;
use theme_settings::ThemeSettings;
use ui::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex};

use workspace::codon_jump_clickable::JumpClickableExt;

use crate::file_manager::{
    ArchiveListing, BinaryInfo, DirEntry, FileManager, ImageInfo, ListingState, PendingInput,
    Preview, TextPreview, format_hex_dump,
};
use crate::prefs::LineMode;
use crate::render::column::{ColumnKind, ColumnTheme, DirtyRows, FmColumnElement};
use crate::render::row::{FmRowElement, RowDisplayState, RowMetrics, resolve_row_theme};
use crate::render::row_glyph_cache::RowGlyphCache;
use crate::render::shaped_line_cache::ShapedLineCache;
use crate::theme::FmThemeStore;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

impl FileManager {
    /// Whether the phase-17 custom-render pipeline is active. The
    /// preference is loaded when the FM is created and then kept in
    /// memory so a paint never performs synchronous config I/O.
    pub(crate) fn custom_render_enabled(&self) -> bool {
        self.custom_render
    }

    /// Clear every per-column row-glyph cache. Called when the
    /// theme changes (composes with `ShapedLineCache::invalidate_for_font`)
    /// or when the directory listing rotates wholesale — the
    /// cached payloads contain resolved colours + shaped glyphs
    /// that must not survive across theme rebuilds.
    #[allow(dead_code)]
    pub(crate) fn clear_row_glyph_caches(&self) {
        self.custom_row_cache_parent.borrow_mut().clear();
        self.custom_row_cache_current.borrow_mut().clear();
        self.custom_row_cache_preview.borrow_mut().clear();
        self.custom_shaped_cache_parent.borrow_mut().clear();
        self.custom_shaped_cache_current.borrow_mut().clear();
        self.custom_shaped_cache_preview.borrow_mut().clear();
    }

    pub(crate) fn refresh_listing_render_snapshots(&mut self) {
        self.refresh_current_listing_render_snapshot();
        self.render_parent_entries = render_entries_snapshot(&self.parent_entries);
    }

    /// Refresh metadata-only enrichment for both listings. Child-count
    /// fill changes painted row payloads but cannot change membership,
    /// file sizes, or the derived mark lookup, so aggregates stay valid.
    pub(crate) fn refresh_listing_enrichment_render_snapshots(&mut self) {
        self.render_entries = render_entries_snapshot(&self.entries);
        self.render_parent_entries = render_entries_snapshot(&self.parent_entries);
    }

    pub(crate) fn refresh_current_listing_render_snapshot(&mut self) {
        self.render_entries = render_entries_snapshot(&self.entries);
        self.render_listing_total_size = self
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.size)
            .sum();
        self.render_listing_revision = self.render_listing_revision.wrapping_add(1);
        self.render_find_key = None;
        self.refresh_mark_render_snapshot();
    }

    pub(crate) fn refresh_preview_render_snapshot(&mut self) {
        self.render_preview_entries = match &self.preview {
            Preview::Directory(entries) => render_entries_snapshot(entries),
            _ => Vec::<Arc<DirEntry>>::new().into(),
        };
    }

    pub(crate) fn refresh_mark_render_snapshot(&mut self) {
        self.render_marked = Arc::new(marked_indices_for_entries(&self.entries, &self.marked));
        let mark_source = self.entries_unfiltered.as_deref().unwrap_or(&self.entries);
        self.render_marked_total_size = mark_source
            .iter()
            .filter(|entry| self.marked.contains(&entry.path))
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.size)
            .sum();
    }
}

fn marked_indices_for_entries(
    entries: &[DirEntry],
    marked: &std::collections::BTreeSet<std::path::PathBuf>,
) -> std::collections::HashSet<usize> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| marked.contains(&entry.path).then_some(index))
        .collect()
}

fn render_entries_snapshot(entries: &[DirEntry]) -> Arc<[Arc<DirEntry>]> {
    entries
        .iter()
        .cloned()
        .map(Arc::new)
        .collect::<Vec<_>>()
        .into()
}

// Free function (not a method) because the `uniform_list` closures
// that drive the virtualized parent + directory-preview columns move
// their captures and can't hold a borrow on the `FileManager` entity.
// All inputs come from the entry + the captured render context — no
// `&self` is needed.
fn render_entry_row(
    entry: &DirEntry,
    index: usize,
    selected: Option<usize>,
    dimmed: bool,
    line_mode: LineMode,
    show_meta: bool,
    cx: &App,
) -> AnyElement {
    // Marks are intrinsically tied to the current column's index space.
    // `render_entry_row` is only called for the parent and preview
    // columns, where applying the marked-set would erroneously
    // highlight rows that happen to share an index with a marked
    // current-column entry. The current column inlines its own
    // marked-row rendering in the `uniform_list` closure.
    let is_selected = selected == Some(index);
    let theme = cx.theme();
    let selected_bg = theme.colors().ghost_element_selected;

    // Dimmed columns (parent + preview-of-directory) always render
    // muted regardless of filetype; otherwise consult the theme
    // overlay so the color reflects extension/directory/dotfile.
    let text_color = if dimmed {
        Color::Muted
    } else {
        filetype_color(entry, cx)
    };

    // `icon_path` is populated by `populate_icon_paths` on the
    // foreground after every listing arrives. The outer Option
    // distinguishes "not yet populated" (renders fallback) from
    // "populated with no specific icon" (`Some(None)`, also renders
    // fallback) — without the cache this used to call
    // `FileIcons::get_icon` per row per paint, doing suffix-split +
    // multi-stage hashmap lookups every time.
    let resolved_icon = entry.icon_path.as_ref().and_then(|o| o.clone());
    let fallback = if entry.is_dir {
        IconName::Folder
    } else {
        IconName::File
    };
    let icon_element = match resolved_icon {
        Some(icon_path) => Icon::from_path(icon_path)
            .size(IconSize::Small)
            .color(Color::Muted)
            .into_any_element(),
        None => Icon::new(fallback)
            .size(IconSize::Small)
            .color(Color::Muted)
            .into_any_element(),
    };

    let symlink_indicator = entry.is_symlink;
    let (git_glyph, git_color, git_filename_color) = git_status_palette(entry.git_status);
    // Git status wins over filetype for the filename tint when the
    // entry is dirty/untracked/etc; on clean entries the filetype
    // overlay (or muted-for-dimmed) carries the color.
    let text_color = git_filename_color.unwrap_or(text_color);

    // Precomputed at DirEntry construction (see `build_entry_labels`)
    // — clones an `Arc` rather than formatting fresh on every paint.
    let meta = if show_meta {
        entry.labels.meta[line_mode.idx()].clone()
    } else {
        None
    };

    h_flex()
        .w_full()
        .pl(px(4.))
        .pr(px(1.))
        .py(px(1.))
        .gap(px(4.))
        .when(is_selected, |d| d.bg(selected_bg))
        .child(
            div().w(px(12.)).flex_none().child(
                Label::new(SharedString::new_static(git_glyph))
                    .size(LabelSize::Small)
                    .color(git_color),
            ),
        )
        .child(icon_element)
        .child(
            div().flex_1().min_w_0().child(
                Label::new(entry.labels.name.clone())
                    .size(LabelSize::Small)
                    .color(text_color)
                    .single_line(),
            ),
        )
        .when(symlink_indicator, |el| {
            el.child(
                Icon::new(IconName::ArrowUpRight)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            )
        })
        .when_some(meta, |el, text| {
            el.child(
                div().w(px(META_COLUMN_WIDTH)).flex_none().child(
                    Label::new(text)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .single_line(),
                ),
            )
        })
        .into_any_element()
}

/// Per-row state shared by the legacy `render_entry_row` and the
/// custom `FmRowElement` paths.
#[derive(Clone, Copy)]
pub(crate) struct RowCallSite {
    pub dimmed: bool,
    pub show_meta: bool,
    pub line_mode: LineMode,
    pub custom_render: bool,
}

/// Build the `AnyElement` for a single row, picking either the
/// custom `FmRowElement` path (when `custom_render = true`) or the
/// legacy nested-Div path (the default until the harness stabilises).
///
/// The shaped-line cache is owned by the FM entity and shared across
/// rows and frames via `Rc<RefCell<_>>`; the same cache serves the
/// custom and legacy paths for a column.
pub(crate) fn build_entry_row(
    entry: &DirEntry,
    index: usize,
    selected_index: Option<usize>,
    is_marked: bool,
    site: RowCallSite,
    cache: &Rc<RefCell<ShapedLineCache>>,
    cx: &App,
) -> AnyElement {
    if !site.custom_render {
        return render_entry_row(
            entry,
            index,
            selected_index,
            site.dimmed,
            site.line_mode,
            site.show_meta,
            cx,
        );
    }

    let theme = resolve_row_theme(cx);
    let font_size = theme::theme_settings(cx).ui_font_size(cx) * 0.85;
    let metrics = RowMetrics::standard(font_size);
    let meta_text = if site.show_meta {
        entry.labels.meta[site.line_mode.idx()].clone()
    } else {
        None
    };

    let row = FmRowElement {
        entry: Arc::new(entry.clone()),
        row_index: index,
        state: RowDisplayState {
            is_selected: selected_index == Some(index),
            is_marked,
            is_focused_row: selected_index == Some(index),
            zebra_stripe: false,
        },
        metrics,
        theme,
        meta_text,
        shaped_line_cache: cache.clone(),
    };
    row.into_any_element()
}

/// Build an `FmColumnElement` for a slice of entries. Returns
/// `None` when the custom render path isn't active so the caller
/// falls back to `uniform_list`.
pub(crate) fn build_fm_column(
    entries: Arc<[Arc<DirEntry>]>,
    selection: Option<usize>,
    marks: Arc<std::collections::HashSet<usize>>,
    site: RowCallSite,
    column_kind: ColumnKind,
    scroll_offset: &Rc<RefCell<f32>>,
    dirty_rows: &Rc<RefCell<DirtyRows>>,
    row_glyph_cache: &Rc<RefCell<RowGlyphCache>>,
    shaped_line_cache: &Rc<RefCell<ShapedLineCache>>,
    cx: &App,
) -> FmColumnElement {
    let row_theme = resolve_row_theme(cx);
    let font_size = theme::theme_settings(cx).ui_font_size(cx) * 0.85;
    let metrics = RowMetrics::standard(font_size);
    let colors = theme::ActiveTheme::theme(cx).colors();
    let column_theme = Arc::new(ColumnTheme {
        row: row_theme,
        scrollbar_track: colors.scrollbar_track_background,
        scrollbar_thumb: colors.scrollbar_thumb_background,
    });

    FmColumnElement {
        column_kind,
        entries,
        selection,
        marks,
        theme: column_theme,
        row_metrics: metrics,
        line_mode: site.line_mode,
        scroll_offset: scroll_offset.clone(),
        shaped_line_cache: shaped_line_cache.clone(),
        row_glyph_cache: row_glyph_cache.clone(),
        dirty_rows: dirty_rows.clone(),
        dimmed: site.dimmed,
    }
}

/// Resolve and cache the icon path for each not-yet-populated entry.
/// `FileIcons::get_icon` needs `&App`, so this must run on the
/// foreground — but the cost is amortized to once per listing-load
/// rather than per row per paint. Idempotent: entries with a cached
/// value are skipped, so callers can re-invoke freely after partial
/// updates (e.g. `spawn_child_count_fill`).
pub(crate) fn populate_icon_paths(entries: &mut [DirEntry], cx: &App) {
    for entry in entries {
        if entry.icon_path.is_some() {
            continue;
        }
        let resolved = if entry.is_dir {
            file_icons::FileIcons::get_folder_icon(false, &entry.path, cx)
        } else {
            file_icons::FileIcons::get_icon(&entry.path, cx)
        };
        entry.icon_path = Some(resolved);
    }
}

impl FileManager {
    fn render_column_static(
        &self,
        entries: Arc<[Arc<DirEntry>]>,
        dimmed: bool,
        show_meta: bool,
        list_id: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let line_mode = self.line_mode;
        let custom_render = self.custom_render_enabled();
        let site = RowCallSite {
            dimmed,
            show_meta,
            line_mode,
            custom_render,
        };
        // Determine which scroll/dirty/cache cells to use based on
        // `list_id`. The two static-column call sites pass
        // "fm-parent-list" or "fm-preview-dir-list" — anything else
        // lands on the preview-column cells.
        let (scroll_cell, dirty_cell, glyph_cache, shaped_cache, kind) =
            if list_id == "fm-parent-list" {
                (
                    &self.custom_scroll_parent,
                    &self.custom_dirty_parent,
                    &self.custom_row_cache_parent,
                    &self.custom_shaped_cache_parent,
                    ColumnKind::Parent,
                )
            } else {
                (
                    &self.custom_scroll_preview,
                    &self.custom_dirty_preview,
                    &self.custom_row_cache_preview,
                    &self.custom_shaped_cache_preview,
                    ColumnKind::Preview,
                )
            };

        if custom_render {
            return build_fm_column(
                entries,
                None,
                Arc::new(Default::default()),
                site,
                kind,
                scroll_cell,
                dirty_cell,
                glyph_cache,
                shaped_cache,
                cx,
            )
            .into_any_element();
        }

        let cache = shaped_cache.clone();
        // Virtualized: a directory whose listing happens to be the
        // user's parent dir can easily run into the hundreds (think
        // `~/Projects/` or `node_modules/`). Eager `.children(...)`
        // materialized every row every paint; `uniform_list` only
        // builds the rows currently visible inside the column's
        // viewport. No explicit bg — the root container paints the
        // panel color once and the columns inherit it.
        uniform_list(list_id, entries.len(), move |range, _window, cx| {
            range
                .map(|i| build_entry_row(&entries[i], i, None, false, site, &cache, cx))
                .collect()
        })
        .size_full()
        .py(px(2.))
        .into_any_element()
    }

    fn render_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let line_mode = self.line_mode;
        let preview_surface = if matches!(&self.preview, Preview::Text(_)) {
            cx.theme().colors().editor_background
        } else {
            cx.theme().colors().panel_background
        };

        // Directory previews use the persistent Arc snapshot directly,
        // avoiding a full `Vec<DirEntry>` clone on every frame. Other
        // variants retain the old owned-snapshot path because the text
        // branch needs a mutable borrow to build/reuse its editor.
        let body: AnyElement = if matches!(&self.preview, Preview::Directory(_)) {
            let entries = self.render_preview_entries.clone();
            let custom_render = self.custom_render_enabled();
            let site = RowCallSite {
                dimmed: true,
                show_meta: true,
                line_mode,
                custom_render,
            };
            if custom_render {
                build_fm_column(
                    entries,
                    None,
                    Arc::new(Default::default()),
                    site,
                    ColumnKind::Preview,
                    &self.custom_scroll_preview,
                    &self.custom_dirty_preview,
                    &self.custom_row_cache_preview,
                    &self.custom_shaped_cache_preview,
                    cx,
                )
                .into_any_element()
            } else {
                let cache = self.custom_shaped_cache_preview.clone();
                // Virtualized: preview can render thousands of children
                // when the user lands on a big directory (`node_modules`,
                // `~/Downloads`). Without `uniform_list`, every
                // navigation rebuilt every row.
                uniform_list(
                    "fm-preview-dir-list",
                    entries.len(),
                    move |range, _window, cx| {
                        range
                            .map(|i| build_entry_row(&entries[i], i, None, false, site, &cache, cx))
                            .collect()
                    },
                )
                .size_full()
                .into_any_element()
            }
        } else {
            match self.preview.clone() {
                Preview::Directory(_) => unreachable!("directory preview handled above"),
                Preview::Text(text) => {
                    render_text_preview(self, &text, window, cx).into_any_element()
                }
                Preview::Archive(listing) => render_archive_preview(&listing).into_any_element(),
                Preview::Image(info) => render_image_preview(&info).into_any_element(),
                Preview::Binary(info) => render_binary_preview(&info, cx).into_any_element(),
                Preview::Empty => div()
                    .child(
                        div().px(px(8.)).child(
                            Label::new("[empty]")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .into_any_element(),
            }
        };

        // `size_full` (not `flex_1`) — the preview is mounted inside a
        // `div().w(...).h_full()` which is NOT a flex parent, so
        // `flex_1` would be a no-op and a `uniform_list` body (which
        // measures its viewport from its own bounds) would collapse to
        // zero rows.
        v_flex()
            .size_full()
            .overflow_hidden()
            .py(px(2.))
            // Text previews eventually mount a full Editor, whose native
            // surface is `editor_background`. Paint that surface from the
            // first fallback frame so the padded editor never appears as an
            // inset card with a panel-colored border around it.
            .bg(preview_surface)
            .child(body)
            .into_any_element()
    }

    fn render_input_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let Some(pending) = &self.pending_input else {
            return div().into_any_element();
        };

        // Owned so the `ConfirmOverwrite` arm can produce a fresh string.
        let (label, value): (&str, std::borrow::Cow<'_, str>) = match pending {
            PendingInput::CreateFile(s) => ("new file: ", s.as_str().into()),
            PendingInput::CreateDirectory(s) => ("new dir: ", s.as_str().into()),
            PendingInput::Rename { new_name, .. } => ("rename: ", new_name.as_str().into()),
            PendingInput::Filter => ("filter: ", self.filter_query.as_str().into()),
            PendingInput::ConfirmOverwrite { plan, .. } => {
                let conflicts = plan.iter().filter(|e| e.destination_exists).count();
                let total = plan.len();
                (
                    "overwrite? ",
                    format!("{conflicts}/{total} target(s) exist — y/N").into(),
                )
            }
            PendingInput::ConfirmDeleteMarked { targets } => {
                let count = targets.len();
                ("delete? ", format!("{count} entries to trash — y/N").into())
            }
            PendingInput::ConfirmSkipTrashDelete { targets } => {
                let count = targets.len();
                (
                    "skip-trash? ",
                    format!("permanently delete {count} entries — y/N").into(),
                )
            }
            PendingInput::BulkRename { pattern, targets } => {
                let count = targets.len();
                (
                    "bulk rename: ",
                    format!("{pattern}   ({count} entries, use {{}} as counter)").into(),
                )
            }
            PendingInput::GotoPath { query } => (":cd ", query.as_str().into()),
            PendingInput::Chmod { input, targets } => {
                let count = targets.len();
                (
                    "chmod: ",
                    format!("{input}   ({count} entries — octal or symbolic)").into(),
                )
            }
            PendingInput::FindForward { query, .. } => ("find: ", query.as_str().into()),
            PendingInput::FindBackward { query, .. } => ("find?: ", query.as_str().into()),
            PendingInput::ContentSearchQuery(query) => ("rg: ", query.as_str().into()),
            PendingInput::ShellBlocking { input } => ("! ", input.as_str().into()),
            PendingInput::ShellAsync { input } => ("; ", input.as_str().into()),
        };

        let theme = cx.theme();

        h_flex()
            .px(px(8.))
            .py(px(2.))
            .bg(theme.colors().editor_background)
            .border_t_1()
            .border_color(theme.colors().border)
            .child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
            .child(
                Label::new(format!("{value}▏"))
                    .size(LabelSize::Small)
                    .color(Color::Default),
            )
            .into_any_element()
    }
}

impl Render for FileManager {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Render-trace harness: custom columns add prepaint/paint/cache
        // counters later in this frame. At the end of this method we
        // schedule a post-frame callback which records the completed
        // wall time and the real aggregate counters.
        let render_started = std::time::Instant::now();
        // `into_any_element()` is applied eagerly because Rust 2024's
        // capture rules treat `impl IntoElement` returned by `&self` /
        // `&mut self` methods as borrowing `*self` for the element's
        // entire lifetime, which would otherwise conflict with the
        // `&mut self` borrow needed by `render_preview`.
        // The parent column is fixed-width on the left and shares a
        // 90px meta gutter with the rest of the rows. When the column
        // gets narrow (FM in a small split, or preview-fraction nudged
        // toward 0.80), that gutter starves the filename. Drop the
        // gutter once the column can no longer afford it — names
        // always have priority over subitem-count/size hints in the
        // dimmed context column. Width is from the previous paint via
        // `on_children_prepainted`; 0.0 (first paint) shows meta.
        let parent_show_meta =
            self.parent_col_width == 0.0 || self.parent_col_width >= PARENT_META_MIN_WIDTH;
        // Responsive: skip building columns that won't be rendered.
        // Treat 0.0 (pre-first-paint) as "wide enough" so the
        // initial layout matches what the user has scrolled into
        // view, avoiding a one-frame flash of a stripped layout.
        let total_for_layout = self.fm_total_width;
        let show_parent_for_build =
            total_for_layout == 0.0 || total_for_layout >= HIDE_PARENT_BELOW;
        let show_preview_for_build =
            total_for_layout == 0.0 || total_for_layout >= HIDE_PREVIEW_BELOW;
        let parent_col = if show_parent_for_build {
            Some(
                self.render_column_static(
                    self.render_parent_entries.clone(),
                    true,
                    parent_show_meta,
                    "fm-parent-list",
                    cx,
                )
                .into_any_element(),
            )
        } else {
            None
        };
        let preview_col = if show_preview_for_build {
            Some(self.render_preview(window, cx))
        } else {
            None
        };
        let input_bar = self.render_input_bar(cx).into_any_element();

        let theme = cx.theme();
        let border_color = theme.colors().border;
        // Single unified background for the whole panel — columns,
        // preview, and chrome all paint on the same surface so the FM
        // reads as one panel, not three cards floating over the window.
        let panel_bg = theme.colors().panel_background;
        let dir_display = self.current_dir.display().to_string();
        let entry_count = self.entries.len();
        let listing_ready = self.listing_state.is_ready();
        let listing_state_banner =
            render_listing_state_banner(&self.listing_state, entry_count, cx);
        let marked_count = self.marked.len();
        let selected_index = self.selected_index;

        let filter_active = !self.filter_query.is_empty();
        let filter_committed =
            filter_active && !matches!(self.pending_input, Some(PendingInput::Filter));
        let filter_query = self.filter_query.clone();
        let focused_entry = self.entries.get(self.selected_index).cloned();
        let focused_child_count = focused_entry.as_ref().and_then(|e| {
            if e.is_dir {
                match &self.preview {
                    Preview::Directory(children) => Some(children.len()),
                    _ => None,
                }
            } else {
                None
            }
        });
        let marked_total_size = self.render_marked_total_size;
        let listing_total_size = self.render_listing_total_size;
        let bottom_bar_state = BottomBarState {
            entry: focused_entry,
            child_count: focused_child_count,
            marked_count,
            marked_total_size,
            listing_total_size,
            listing_count: entry_count,
            visual_mode: self.visual_anchor.is_some(),
            selected_index,
        };
        // Pending-input contextual hints outrank the Cmd-held overlay;
        // when neither applies, the bar falls back to entry info.
        let bottom_left_mode = if let Some(hints) = contextual_help_hints(self) {
            BottomBarLeft::ContextualHints(hints)
        } else if self.cmd_only_held {
            BottomBarLeft::CmdShortcuts(general_shortcut_hints())
        } else {
            BottomBarLeft::Info
        };
        let error_message = self.error_message.clone();
        let shell_banner = self.shell_running.as_ref().map(|r| r.command.clone());

        // Header-chip inputs — sourced from existing panel state. Match
        // counts are computed here once so the chip doesn't pay for an
        // O(n) scan on every cell render.
        let find_pending = self.pending_input.as_ref().and_then(|p| match p {
            PendingInput::FindForward { query, .. } | PendingInput::FindBackward { query, .. } => {
                Some(query.clone())
            }
            _ => None,
        });
        let find_active_pattern = find_pending.or_else(|| self.last_find_pattern.clone());
        let find_match_count = if let Some(needle) = find_active_pattern.as_ref() {
            let key = (self.render_listing_revision, needle.clone());
            if self.render_find_key.as_ref() != Some(&key) {
                self.render_find_match_count = count_find_matches(&self.entries, needle);
                self.render_find_key = Some(key);
            }
            self.render_find_match_count
        } else {
            0
        };
        let top_bar = TopBarState {
            dir_path: dir_display,
            sort: self.sort,
            reverse: self.reverse,
            filter_query: if filter_active {
                Some(self.filter_query.clone())
            } else {
                None
            },
            find_query: find_active_pattern,
            find_match_count,
            show_hidden: self.show_hidden,
        };

        let entries = self.render_entries.clone();
        let marked = self.render_marked.clone();
        let this = cx.entity().downgrade();
        let focus = self.focus_handle.clone();
        let line_mode = self.line_mode;
        let custom_render = self.custom_render_enabled();
        let selected_idx_for_current = self.selected_index;

        // Phase-17 custom render path for the current column.
        let current_col: AnyElement = if custom_render {
            let site = RowCallSite {
                dimmed: false,
                show_meta: true,
                line_mode,
                custom_render: true,
            };
            build_fm_column(
                entries,
                Some(selected_idx_for_current),
                marked,
                site,
                ColumnKind::Current,
                &self.custom_scroll_current,
                &self.custom_dirty_current,
                &self.custom_row_cache_current,
                &self.custom_shaped_cache_current,
                cx,
            )
            .into_any_element()
        } else {
            let current_col = uniform_list("file-list", entries.len(), {
                move |range, _window, cx| {
                    let theme = cx.theme();
                    // Cursor row uses the active token (vs the dimmer
                    // `ghost_element_selected`) so the focused row pops at
                    // a glance — and stays distinguishable when it's also
                    // a marked row (the 2px accent stripe survives on top).
                    let selected_bg = theme.colors().ghost_element_active;

                    range
                        .map(|i| {
                            let entry = &entries[i];
                            let is_selected = i == selected_index;
                            let is_marked = marked.contains(&i);

                            // Marked rows keep the accent tint so the
                            // "marked" cue is clearly the priority signal;
                            // otherwise the filetype overlay drives the
                            // filename color.
                            let text_color = if is_marked {
                                Color::Accent
                            } else {
                                filetype_color(entry, cx)
                            };

                            // Cached by `populate_icon_paths`; see `render_entry_row`
                            // for the cache semantics.
                            let resolved_icon = entry.icon_path.as_ref().and_then(|o| o.clone());
                            let fallback = if entry.is_dir {
                                IconName::Folder
                            } else {
                                IconName::File
                            };
                            let icon_element = match resolved_icon {
                                Some(p) => Icon::from_path(p)
                                    .size(IconSize::Small)
                                    .color(Color::Muted)
                                    .into_any_element(),
                                None => Icon::new(fallback)
                                    .size(IconSize::Small)
                                    .color(Color::Muted)
                                    .into_any_element(),
                            };

                            let marked_bg = theme.colors().ghost_element_hover;
                            let this = this.clone();
                            let focus = focus.clone();
                            let (git_glyph, git_color, git_filename_color) =
                                git_status_palette(entry.git_status);
                            // Marked rows keep their accent tint; git
                            // status overrides the filetype color on dirty
                            // entries; otherwise filetype color carries.
                            let text_color = if is_marked {
                                text_color
                            } else {
                                git_filename_color.unwrap_or(text_color)
                            };
                            // Precomputed at DirEntry construction; clone is a
                            // cheap `Arc` bump rather than a fresh format.
                            let meta = entry.labels.meta[line_mode.idx()].clone();

                            // Marked rows get a 2px left-edge stripe in
                            // the accent color in addition to the bg tint.
                            // The stripe survives when the cursor row also
                            // tints — the bg color of the row swallows the
                            // marked alpha but not the explicit stripe.
                            let stripe_color = theme.colors().text_accent;
                            div()
                                .id(("file-entry", i))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .pr(px(1.))
                                        .py(px(1.))
                                        .gap(px(4.))
                                        .when(is_marked && !is_selected, |d| d.bg(marked_bg))
                                        .when(is_selected, |d| d.bg(selected_bg))
                                        // Left edge: 2px stripe slot (in
                                        // accent when marked, transparent
                                        // otherwise) followed by 2px of
                                        // breathing room. Keeps the row's
                                        // text aligned regardless of mark
                                        // state.
                                        .child(if is_marked {
                                            div()
                                                .w(px(2.))
                                                .flex_none()
                                                .bg(stripe_color)
                                                .into_any_element()
                                        } else {
                                            div().w(px(2.)).flex_none().into_any_element()
                                        })
                                        .child(div().w(px(2.)).flex_none())
                                        .child(
                                            div().w(px(12.)).flex_none().child(
                                                Label::new(SharedString::new_static(git_glyph))
                                                    .size(LabelSize::Small)
                                                    .color(git_color),
                                            ),
                                        )
                                        .child(icon_element)
                                        .child(
                                            div().flex_1().min_w_0().child(
                                                Label::new(entry.labels.name.clone())
                                                    .size(LabelSize::Small)
                                                    .color(text_color)
                                                    .when(is_selected, |l| {
                                                        l.weight(FontWeight::BOLD)
                                                    })
                                                    .single_line(),
                                            ),
                                        )
                                        .when(entry.is_symlink, |el| {
                                            el.child(
                                                Icon::new(IconName::ArrowUpRight)
                                                    .size(IconSize::XSmall)
                                                    .color(Color::Muted),
                                            )
                                        })
                                        .when_some(meta, |el, text| {
                                            el.child(
                                                div().w(px(META_COLUMN_WIDTH)).flex_none().child(
                                                    Label::new(text)
                                                        .size(LabelSize::Small)
                                                        .color(Color::Muted)
                                                        .single_line(),
                                                ),
                                            )
                                        }),
                                )
                                .on_click({
                                    let this = this.clone();
                                    let focus = focus.clone();
                                    move |_event, window, cx| {
                                        focus.focus(window, cx);
                                        this.update(cx, |fm, cx| {
                                            fm.selected_index = i;
                                            fm.request_preview_update(cx);
                                            cx.notify();
                                        })
                                        .ok();
                                    }
                                })
                                .jump_target(move |window, cx| {
                                    focus.focus(window, cx);
                                    this.update(cx, |fm, cx| {
                                        fm.selected_index = i;
                                        fm.request_preview_update(cx);
                                        cx.notify();
                                    })
                                    .ok();
                                })
                                .into_any_element()
                        })
                        .collect()
                }
            })
            .size_full()
            .py(px(2.))
            .track_scroll(&self.scroll_handle);
            current_col.into_any_element()
        };

        let panel = v_flex()
            .size_full()
            .bg(panel_bg)
            .key_context(self.dispatch_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_cancel))
            .on_action(cx.listener(Self::handle_choose_opener))
            .on_action(cx.listener(Self::handle_ranger))
            .on_action(cx.listener(Self::handle_object_next))
            .on_action(cx.listener(Self::handle_object_prev))
            .on_action(cx.listener(Self::handle_inner_container))
            .on_action(cx.listener(Self::handle_around_container))
            .on_action(cx.listener(Self::handle_select_all_kind))
            .on_action(
                cx.listener(|this, _: &crate::file_manager::SortByName, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Name, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::file_manager::SortBySize, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Size, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::file_manager::SortByMtime, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Mtime, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::file_manager::SortByBtime, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Btime, window, cx);
                }),
            )
            .on_action(cx.listener(
                |this, _: &crate::file_manager::SortByExtension, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Extension, window, cx);
                },
            ))
            .on_action(
                cx.listener(|this, _: &crate::file_manager::SortByNatural, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Natural, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::file_manager::SortByRandom, window, cx| {
                    this.set_sort_mode(crate::prefs::SortMode::Random, window, cx);
                }),
            )
            .on_action(cx.listener(
                |this, _: &crate::file_manager::ToggleSortReverse, window, cx| {
                    this.toggle_sort_reverse(window, cx);
                },
            ))
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .child(render_top_bar(&top_bar, cx))
            .when_some(listing_state_banner, |this, banner| this.child(banner))
            .when(filter_committed, |this| {
                this.child(
                    h_flex()
                        .px(px(8.))
                        .py(px(2.))
                        .gap(px(6.))
                        .bg(theme.colors().editor_background)
                        .border_b_1()
                        .border_color(border_color)
                        .child(
                            Icon::new(IconName::Filter)
                                .size(IconSize::XSmall)
                                .color(Color::Accent),
                        )
                        .child(
                            Label::new(filter_query.clone())
                                .size(LabelSize::Small)
                                .color(Color::Accent),
                        )
                        .child(
                            Label::new("(Esc to clear, F to edit)")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
            })
            .child({
                // Responsive column visibility: at wide widths show
                // all three columns; below `HIDE_PARENT_BELOW` drop
                // the parent (leftmost) column so preview keeps its
                // space; below `HIDE_PREVIEW_BELOW` drop preview too
                // and only show the current directory. 0.0 (first
                // paint, before measurement) is treated as wide so we
                // don't briefly flash a stripped-down layout.
                let total = self.fm_total_width;
                let show_parent = total == 0.0 || total >= HIDE_PARENT_BELOW;
                let show_preview = total == 0.0 || total >= HIDE_PREVIEW_BELOW;

                h_flex()
                    .flex_1()
                    .min_h_0()
                    .when(!listing_ready, |row| row.opacity(0.55))
                    .on_children_prepainted({
                        // Capture two things from the painted column
                        // row: the parent column width (drives the
                        // meta-gutter decision in render_entry) and
                        // the total span (drives the responsive
                        // hide-column decision above). Only schedule
                        // a notify when either crosses a 1px boundary
                        // — without the gate, every paint would
                        // re-enter the entity and we'd render in a
                        // tight loop.
                        let entity = cx.entity().downgrade();
                        let prev_parent = self.parent_col_width;
                        let prev_total = self.fm_total_width;
                        let show_parent_now = show_parent;
                        move |bounds, _window, cx| {
                            if bounds.is_empty() {
                                return;
                            }
                            // SAFETY: bounds.is_empty() returned false above.
                            let (Some(first), Some(last)) = (bounds.first(), bounds.last()) else {
                                return;
                            };
                            let new_total =
                                f32::from(last.origin.x + last.size.width - first.origin.x);
                            // When the parent column isn't rendered,
                            // there's no parent width to capture; keep
                            // the last measured value so toggling back
                            // doesn't briefly drop the meta gutter.
                            let new_parent = if show_parent_now {
                                f32::from(first.size.width)
                            } else {
                                prev_parent
                            };
                            let parent_changed = (new_parent - prev_parent).abs() >= 1.0;
                            let total_changed = (new_total - prev_total).abs() >= 1.0;
                            if !parent_changed && !total_changed {
                                return;
                            }
                            if let Some(entity) = entity.upgrade() {
                                entity.update(cx, |fm, cx| {
                                    fm.parent_col_width = new_parent;
                                    fm.fm_total_width = new_total;
                                    cx.notify();
                                });
                            }
                        }
                    })
                    .when_some(parent_col, |row, col| {
                        row.child(
                            div()
                                .w(relative(parent_fraction(self.preview_fraction)))
                                .h_full()
                                .overflow_hidden()
                                .border_r_1()
                                .border_color(border_color)
                                .child(col),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .min_h_0()
                            .when(show_preview, |d| d.border_r_1().border_color(border_color))
                            .child(current_col),
                    )
                    .when_some(preview_col, |row, col| {
                        row.child(
                            div()
                                .w(relative(self.preview_fraction))
                                .h_full()
                                .overflow_hidden()
                                .child(col),
                        )
                    })
            })
            .child(input_bar)
            .when_some(shell_banner, |this, cmd| {
                let truncated: String = cmd.chars().take(80).collect();
                this.child(
                    div()
                        .px(px(8.))
                        .py(px(2.))
                        .border_t_1()
                        .border_color(border_color)
                        .bg(theme.colors().editor_background)
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .child(
                                    Label::new("running:")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(
                                    Label::new(truncated)
                                        .size(LabelSize::Small)
                                        .color(Color::Default),
                                )
                                .child(
                                    Label::new("(Esc to terminate)")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                ),
                        ),
                )
            })
            .when_some(error_message, |this, msg| {
                this.child(
                    div()
                        .px(px(8.))
                        .py(px(1.))
                        .border_t_1()
                        .border_color(border_color)
                        .child(Label::new(msg).size(LabelSize::Small).color(Color::Error)),
                )
            })
            .child(render_bottom_bar(&bottom_bar_state, bottom_left_mode, cx));

        let render_build_ms = render_started.elapsed().as_secs_f32() * 1000.0;
        crate::render::trace::schedule_completed_frame(
            window,
            render_started,
            render_build_ms,
            entry_count,
        );

        panel
    }
}

fn render_listing_state_banner(
    state: &ListingState,
    entry_count: usize,
    cx: &App,
) -> Option<AnyElement> {
    let (message, color) = match state {
        ListingState::Loading { path } if entry_count == 0 => {
            (format!("Loading {}…", path.display()), Color::Accent)
        }
        ListingState::Loading { path } => (
            format!(
                "Loading {}… · showing retained rows read-only",
                path.display()
            ),
            Color::Accent,
        ),
        ListingState::Error { path, message } => (
            format!(
                "Couldn’t load {}: {} · Shift-R to retry",
                path.display(),
                message
            ),
            Color::Error,
        ),
        ListingState::Ready { .. } if entry_count == 0 => {
            ("Empty directory".to_string(), Color::Muted)
        }
        ListingState::Ready { .. } => return None,
    };
    let theme = cx.theme();
    Some(
        h_flex()
            .px(px(8.))
            .py(px(2.))
            .bg(theme.colors().editor_background)
            .border_b_1()
            .border_color(theme.colors().border)
            .child(Label::new(message).size(LabelSize::Small).color(color))
            .into_any_element(),
    )
}

fn render_text_preview(
    fm: &mut FileManager,
    text: &TextPreview,
    window: &mut Window,
    cx: &mut Context<FileManager>,
) -> AnyElement {
    // Deferred-editor preview: while the user is moving across
    // rows, render a static text snapshot via plain glyph runs and
    // skip the full `EditorElement::prepaint` →
    // `WindowTextSystem::shape_line` cost. Once the dwell timer
    // has elapsed on the same target path, upgrade to the
    // real editor for cursor / folding / gutter / etc. The static snapshot
    // deliberately uses the editor's surface and typography so this is a
    // content-quality upgrade, not a background/font-size flash. See
    // `REQ:codon/fm-render#c-defer-editor-in-preview`.
    let dwell_complete =
        fm.preview_dwell_upgraded && fm.preview_dwell_path.as_ref() == Some(&text.path);
    if !dwell_complete {
        return render_text_preview_static(text, cx);
    }
    let editor = fm.preview_editor_for(text, window, cx);
    div()
        .size_full()
        .px(px(8.))
        .py(px(2.))
        .bg(cx.theme().colors().editor_background)
        .child(editor)
        .into_any_element()
}

/// Static fallback rendering for the deferred-editor preview. Emits
/// the file's bytes with the same surface, font, size, line height, and
/// foreground as the upgraded editor, without paying the full
/// `EditorElement::prepaint` cost on every `j` / `k` move.
fn render_text_preview_static(text: &TextPreview, cx: &App) -> AnyElement {
    // Cap the static preview to a screenful's worth of lines.
    // Beyond that the user is going to want the real editor (which
    // the dwell timer will deliver after a stable selection).
    const MAX_LINES: usize = 64;
    let lines: Vec<SharedString> = text
        .content
        .lines()
        .take(MAX_LINES)
        .map(|line| SharedString::from(line.to_owned()))
        .collect();
    let settings = ThemeSettings::get_global(cx);
    v_flex()
        .size_full()
        .px(px(8.))
        .py(px(2.))
        .gap(px(0.))
        .overflow_hidden()
        .bg(cx.theme().colors().editor_background)
        .font(settings.buffer_font.clone())
        .text_size(settings.buffer_font_size(cx))
        .line_height(relative(settings.line_height()))
        .text_color(cx.theme().colors().editor_foreground)
        .children(
            lines
                .into_iter()
                .map(|line| div().overflow_hidden().whitespace_nowrap().child(line)),
        )
        .into_any_element()
}

fn render_image_preview(info: &ImageInfo) -> impl IntoElement {
    let dim_label = info
        .dimensions
        .map(|(w, h)| format!("{w}×{h}"))
        .unwrap_or_else(|| "unknown size".to_string());
    let header = format!(
        "{} · {} · {} · {}",
        info.name,
        human_size(info.size),
        info.mime,
        dim_label,
    );

    let fallback_label = header.clone();
    let image_path = info.path.clone();

    v_flex()
        .px(px(8.))
        .py(px(2.))
        .gap(px(4.))
        .size_full()
        .child(
            Label::new(header)
                .size(LabelSize::Small)
                .color(Color::Default),
        )
        .child(
            div().flex_1().min_h_0().child(
                img(image_path)
                    .object_fit(ObjectFit::Contain)
                    .size_full()
                    .with_fallback(move || {
                        div()
                            .px(px(8.))
                            .child(
                                Label::new(SharedString::from(fallback_label.clone()))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .into_any_element()
                    }),
            ),
        )
}

fn render_archive_preview(listing: &ArchiveListing) -> impl IntoElement {
    let mut lines: Vec<String> = listing
        .entries
        .iter()
        .map(|entry| match entry.size {
            Some(size) => format!("{}    {}", entry.name, human_size(size)),
            None => entry.name.clone(),
        })
        .collect();
    if listing.extra > 0 {
        lines.push(format!("… {} more", listing.extra));
    }
    v_flex()
        .px(px(8.))
        .py(px(2.))
        .children(lines.into_iter().map(|line| {
            Label::new(SharedString::from(line))
                .size(LabelSize::Small)
                .color(Color::Muted)
        }))
}

fn render_binary_preview(info: &BinaryInfo, cx: &App) -> impl IntoElement {
    let header = format!("{} · {} · {}", info.name, human_size(info.size), info.mime);
    let type_label = mime_type_label(&info.mime);
    let dump = format_hex_dump(&info.head);
    let dump_lines: Vec<String> = dump.lines().map(|l| l.to_string()).collect();

    v_flex()
        .px(px(8.))
        .py(px(2.))
        .gap(px(2.))
        .child(
            Label::new(header)
                .size(LabelSize::Small)
                .color(Color::Default),
        )
        .child(
            Label::new(SharedString::from(type_label))
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(v_flex().children(dump_lines.into_iter().map(|line| {
            Label::new(SharedString::from(line))
                .size(LabelSize::Small)
                .color(Color::Muted)
                .buffer_font(cx)
        })))
}

/// Human-readable type label for the binary fallback header. Derived
/// from the mime guess so adding a new extension to `mime_guess`
/// upstream just works. The mapping is intentionally coarse — ranger /
/// yazi show roughly this much without invoking external probes, and
/// pulling in `symphonia` / `pdfium` just for a preview line is more
/// weight than the feature warrants.
fn mime_type_label(mime: &str) -> String {
    let (top, sub) = mime.split_once('/').unwrap_or((mime, ""));
    let kind = match top {
        "audio" => "Audio file",
        "video" => "Video file",
        "image" => "Image file",
        "font" => "Font file",
        "text" => "Text file",
        "model" => "3D model",
        "application" => match sub {
            "pdf" => return "PDF document".to_string(),
            "json" | "xml" | "yaml" | "x-yaml" | "toml" => "Structured data",
            "zip" | "x-tar" | "x-7z-compressed" | "x-rar-compressed" | "gzip" | "x-bzip2"
            | "x-xz" | "x-zstd" => "Archive",
            "x-sharedlib"
            | "x-executable"
            | "x-mach-binary"
            | "vnd.microsoft.portable-executable"
            | "wasm" => "Executable / binary",
            "x-font-ttf" | "x-font-otf" | "x-font-woff" => "Font file",
            _ => "Binary data",
        },
        _ => "Binary data",
    };
    if sub.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} ({sub})")
    }
}

/// Parent-column fraction as a function of the preview-column fraction.
/// Stays at 1/4 when preview is at its default 1/3, then scales down
/// proportionally as the preview column grows so the middle column
/// always retains usable width even at the 0.80 ceiling. Below the
/// default preview, parent stays pinned at 1/4 rather than expanding —
/// the middle column absorbs the freed space, which is the column the
/// user is steering with j/k.
pub(crate) fn parent_fraction(preview_fraction: f32) -> f32 {
    let denom = 1.0 - crate::prefs::PREVIEW_FRACTION_DEFAULT;
    if denom <= 0.0 {
        return 0.25;
    }
    let factor = ((1.0 - preview_fraction) / denom).clamp(0.0, 1.0);
    0.25 * factor
}

/// Width in pixels of the right-aligned metadata column. Sized to fit
/// "drwxrwxr-x" comfortably (the widest variant) so columns line up
/// across modes and toggling `M` does not shift the entry text.
pub(crate) const META_COLUMN_WIDTH: f32 = 90.0;

/// Minimum parent-column pixel width at which the dimmed context
/// column still shows the meta gutter. Below this, names get priority
/// and the gutter is dropped. Chosen so the filename has roughly
/// 90px of room left after fixed chrome (~40px: padding + git glyph
/// + icon + gaps) plus the 90px meta gutter — anything narrower
/// makes the filename effectively unreadable.
pub(crate) const PARENT_META_MIN_WIDTH: f32 = 220.0;

/// Minimum FM-row pixel width at which the parent (left) column
/// still renders. Below this the three-column layout collapses
/// to two — parent goes, current + preview remain. Picked so the
/// current column keeps ~250px of usable width at the default
/// preview fraction (1/3 of total) once parent is gone.
pub(crate) const HIDE_PARENT_BELOW: f32 = 640.0;

/// Minimum FM-row pixel width at which the preview (right) column
/// still renders. Below this the layout collapses to a single
/// column — just the current directory. Below ~360px the preview
/// has so little room that wrapping/truncation makes it more
/// noise than signal; ceding the space to the current column is
/// the better trade.
pub(crate) const HIDE_PREVIEW_BELOW: f32 = 360.0;

pub(crate) fn entry_meta_label(entry: &DirEntry, mode: LineMode) -> Option<String> {
    match mode {
        LineMode::None => None,
        LineMode::Size => {
            if entry.is_dir {
                entry.child_count.map(|n| {
                    if n == 1 {
                        "1 item".to_string()
                    } else {
                        format!("{n} items")
                    }
                })
            } else {
                Some(human_size(entry.size))
            }
        }
        LineMode::Mtime => entry.mtime.map(format_relative_time),
        LineMode::Permissions => Some(format_permissions(
            entry.is_dir,
            entry.is_symlink,
            entry.mode,
        )),
        LineMode::Owner => Some(format_owner(entry.uid, entry.gid)),
    }
}

fn format_relative_time(t: SystemTime) -> String {
    let now = SystemTime::now();
    let (sign, dur) = match now.duration_since(t) {
        Ok(d) => ("ago", d),
        Err(e) => ("from now", e.duration()),
    };
    let secs = dur.as_secs();
    let label = if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else if secs < 86400 * 30 {
        format!("{}d", secs / 86400)
    } else if secs < 86400 * 365 {
        format!("{}mo", secs / (86400 * 30))
    } else {
        format!("{}y", secs / (86400 * 365))
    };
    format!("{label} {sign}")
}

fn format_permissions(is_dir: bool, is_symlink: bool, mode: Option<u32>) -> String {
    let Some(mode) = mode else {
        return "----------".to_string();
    };
    let typ = if is_symlink {
        'l'
    } else if is_dir {
        'd'
    } else {
        '-'
    };
    let bits = mode & 0o777;
    let triplet = |shift: u32| -> String {
        let v = (bits >> shift) & 0o7;
        let r = if v & 0o4 != 0 { 'r' } else { '-' };
        let w = if v & 0o2 != 0 { 'w' } else { '-' };
        let x = if v & 0o1 != 0 { 'x' } else { '-' };
        format!("{r}{w}{x}")
    };
    format!("{typ}{}{}{}", triplet(6), triplet(3), triplet(0))
}

fn format_owner(uid: Option<u32>, gid: Option<u32>) -> String {
    match (uid, gid) {
        (Some(u), Some(g)) => format!("{u}:{g}"),
        (Some(u), None) => format!("{u}:?"),
        (None, Some(g)) => format!("?:{g}"),
        (None, None) => "?:?".to_string(),
    }
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes < TB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    }
}

/// Snapshot of the header-chip inputs. Built once per render in
/// `FileManager::render` and handed to `render_header_chips` so the
/// helper stays free of `&FileManager` plumbing.
struct TopBarState {
    dir_path: String,
    sort: crate::prefs::SortMode,
    reverse: bool,
    filter_query: Option<String>,
    find_query: Option<String>,
    find_match_count: usize,
    show_hidden: bool,
}

/// Snapshot of every signal the bottom bar's *info* mode needs. Kept
/// separate so the renderer doesn't need a `FileManager` borrow.
pub(crate) struct BottomBarState {
    pub entry: Option<DirEntry>,
    pub child_count: Option<usize>,
    pub marked_count: usize,
    pub marked_total_size: u64,
    pub listing_total_size: u64,
    pub listing_count: usize,
    pub visual_mode: bool,
    pub selected_index: usize,
}

/// Which content occupies the bottom bar's left half this frame. The
/// renderer picks one of these and the shell (padding / border / bg)
/// stays identical across modes so toggling doesn't reflow.
pub(crate) enum BottomBarLeft {
    /// Default — focused-entry segments (perms / owner / size / mtime /
    /// name).
    Info,
    /// Task-driven hints (open prompt, visual range, marked set).
    /// Outranks `CmdShortcuts` when both apply.
    ContextualHints(Vec<(&'static str, &'static str)>),
    /// General shortcut cheatsheet shown while Cmd is the only modifier
    /// held in the window.
    CmdShortcuts(Vec<(&'static str, &'static str)>),
}

/// Ranger-style info row above the status line: focused entry's
/// permissions / owner / size / mtime, plus listing/selection totals on
/// the right. Reads dense at a glance without crowding the status bar.
pub(crate) fn render_bottom_bar(
    state: &BottomBarState,
    left_mode: BottomBarLeft,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let border_color = theme.colors().border;

    let right_segments: Vec<String> = {
        let position = if state.listing_count > 0 {
            format!("{}/{}", state.selected_index + 1, state.listing_count)
        } else {
            format!("0/{}", state.listing_count)
        };
        if state.marked_count > 0 || state.visual_mode {
            let mut v = Vec::new();
            if state.visual_mode {
                v.push(format!("VISUAL ({})", state.marked_count));
            } else {
                v.push(format!("{} marked", state.marked_count));
            }
            if state.marked_total_size > 0 {
                v.push(human_size(state.marked_total_size));
            }
            v.push(format!(
                "/ {} files ({})",
                state.listing_count,
                human_size(state.listing_total_size),
            ));
            v.push(position);
            v
        } else {
            vec![
                format!(
                    "{} entries ({})",
                    state.listing_count,
                    human_size(state.listing_total_size),
                ),
                position,
            ]
        }
    };

    let left_element: gpui::AnyElement = match left_mode {
        BottomBarLeft::Info => render_bottom_left_info(state).into_any_element(),
        BottomBarLeft::ContextualHints(hints) => {
            render_bottom_left_hints(&hints).into_any_element()
        }
        BottomBarLeft::CmdShortcuts(hints) => render_bottom_left_hints(&hints).into_any_element(),
    };

    h_flex()
        .px(px(8.))
        .py(px(1.))
        .gap(px(8.))
        .border_t_1()
        .border_color(border_color)
        .bg(theme.colors().editor_background)
        .child(left_element)
        .child(
            h_flex()
                .gap(px(6.))
                .children(right_segments.into_iter().map(|s| {
                    Label::new(SharedString::from(s))
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .into_any_element()
                })),
        )
}

/// Default left half — the rich info segments for the focused entry.
/// Mirrors what the standalone rich-info bar used to show.
fn render_bottom_left_info(state: &BottomBarState) -> impl IntoElement {
    let mut left_segments: Vec<String> = Vec::new();
    if let Some(entry) = state.entry.as_ref() {
        let perms = format_permissions(entry.is_dir, entry.is_symlink, entry.mode);
        left_segments.push(perms);
        let owner = format_owner(entry.uid, entry.gid);
        if owner != "?:?" {
            left_segments.push(owner);
        }
        if entry.is_dir {
            match state.child_count {
                Some(n) => left_segments.push(format!("{n} items")),
                None => left_segments.push("dir".to_string()),
            }
        } else {
            left_segments.push(human_size(entry.size));
        }
        if let Some(t) = entry.mtime {
            left_segments.push(format_relative_time(t));
        }
        left_segments.push(entry.name.clone());
    } else {
        left_segments.push("—".to_string());
    }

    h_flex()
        .flex_1()
        .min_w_0()
        .gap(px(6.))
        .children(left_segments.into_iter().map(|s| {
            Label::new(SharedString::from(s))
                .size(LabelSize::Small)
                .color(Color::Muted)
                .into_any_element()
        }))
}

/// Hint left half — `key — verb` pairs, used by both the contextual
/// overlay (open prompt / visual / marks) and the Cmd-held cheatsheet.
fn render_bottom_left_hints(hints: &[(&'static str, &'static str)]) -> impl IntoElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .gap(px(10.))
        .children(hints.iter().map(|(k, v)| {
            h_flex()
                .gap(px(3.))
                .child(
                    Label::new(SharedString::new_static(k))
                        .size(LabelSize::Small)
                        .color(Color::Accent),
                )
                .child(
                    Label::new(SharedString::new_static(v))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        }))
}

/// Pick which hints to surface based on what the user is currently
/// doing. Order is "most relevant first" so the leftmost slots earn
/// their pixels.
/// Hints surfaced by the bottom bar when the FM is in a task-driven
/// state — an open prompt, an active visual-line range, or a non-empty
/// marked set. Returns `None` when nothing contextual applies, so the
/// caller can fall back to the default info segments.
pub(crate) fn contextual_help_hints(fm: &FileManager) -> Option<Vec<(&'static str, &'static str)>> {
    if fm.pending_input.is_some() {
        return Some(vec![
            ("⏎", "confirm"),
            ("Esc", "cancel"),
            ("Tab", "complete"),
        ]);
    }
    if fm.visual_anchor.is_some() {
        return Some(vec![
            ("j/k", "extend"),
            ("⏎/Esc", "commit"),
            ("y", "yank"),
            ("d", "cut"),
            ("D", "trash"),
        ]);
    }
    if !fm.marked.is_empty() {
        return Some(vec![
            ("p", "paste"),
            ("y", "yank"),
            ("d", "cut"),
            ("D", "delete"),
            ("gcw", "bulk-rename"),
            ("uv", "clear marks"),
        ]);
    }
    None
}

/// Static "what can I do here" cheatsheet shown in the bottom bar
/// while Cmd is the only modifier held. Same key/verb format as
/// `contextual_help_hints` so the renderer can share one helper.
pub(crate) fn general_shortcut_hints() -> Vec<(&'static str, &'static str)> {
    vec![
        ("hjkl", "nav"),
        ("⏎", "open"),
        ("a/A", "new file/dir"),
        ("r", "rename"),
        ("d", "cut"),
        ("y", "copy"),
        ("v", "mark"),
        ("/", "find"),
        ("F", "filter"),
        ("zh", "hidden"),
        ("M", "info col"),
        (";:", "cmd"),
    ]
}

/// Count case-insensitive substring matches of `needle` against every
/// entry name. Used to populate the find chip's `(N)` suffix — runs
/// once per render so it's cheap relative to laying out the panel.
fn count_find_matches(entries: &[DirEntry], needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let lowered = needle.to_lowercase();
    entries
        .iter()
        .filter(|e| e.name.to_lowercase().contains(&lowered))
        .count()
}

/// Short label for a sort mode + direction arrow, matching the
/// keymap-default verb names so the chip reads as a direct echo of the
/// `,m`/`,s`/etc bindings.
fn sort_chip_label(mode: crate::prefs::SortMode, reverse: bool) -> String {
    use crate::prefs::SortMode;
    let base = match mode {
        SortMode::Name => "name",
        SortMode::Size => "size",
        SortMode::Mtime => "mtime",
        SortMode::Btime => "btime",
        SortMode::Extension => "ext",
        SortMode::Random => "rand",
        SortMode::Natural => "nat",
    };
    // Arrow direction reads as "what would `,r` show" — ascending is
    // `↓` (top-of-list smaller / earlier), reversed flips to `↑`.
    let arrow = if reverse { "↑" } else { "↓" };
    match mode {
        SortMode::Random => base.to_string(),
        _ => format!("{base} {arrow}"),
    }
}

/// Truncate `s` to `max` chars, appending an ellipsis when clipped, so
/// long filter/find patterns can't stretch the chip past its budget.
fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Top bar: current directory path on the left + active chips on the
/// right. Sort is always present (dim when at the default `name ↓`
/// setting); filter / find / hidden chips appear only when their
/// state is non-default. The path uses `min_w_0` + `single_line` so a
/// long path truncates rather than pushing the chips off-screen.
fn render_top_bar(state: &TopBarState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let status = theme.status();
    let border_color = theme.colors().border;

    let sort_is_default = matches!(state.sort, crate::prefs::SortMode::Name) && !state.reverse;
    let sort_label = sort_chip_label(state.sort, state.reverse);
    let sort_chip = chip(
        sort_label,
        if sort_is_default {
            theme.colors().element_background
        } else {
            theme.colors().element_selected
        },
        if sort_is_default {
            Color::Muted
        } else {
            Color::Accent
        },
    );

    let mut chips: Vec<gpui::AnyElement> = vec![sort_chip.into_any_element()];

    if let Some(pattern) = state.filter_query.as_ref() {
        let label = format!("filter:{}", truncate_label(pattern, 20));
        chips.push(chip(label, status.warning_background, Color::Warning).into_any_element());
    }

    if let Some(pattern) = state.find_query.as_ref() {
        let label = format!(
            "find:{} ({})",
            truncate_label(pattern, 20),
            state.find_match_count
        );
        chips.push(chip(label, status.info_background, Color::Info).into_any_element());
    }

    if state.show_hidden {
        chips.push(
            chip(
                ".".to_string(),
                theme.colors().element_background,
                Color::Muted,
            )
            .into_any_element(),
        );
    }

    h_flex()
        .px(px(8.))
        .py(px(2.))
        .gap(px(6.))
        .border_b_1()
        .border_color(border_color)
        .bg(theme.colors().editor_background)
        .child(
            div().flex_1().min_w_0().child(
                Label::new(SharedString::from(state.dir_path.clone()))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .single_line(),
            ),
        )
        .child(h_flex().gap(px(4.)).children(chips))
}

/// One chip element: rounded, padded, single-line label. Used by every
/// header-chip variant — only the bg + fg colors vary.
fn chip(label: impl Into<SharedString>, bg: gpui::Hsla, fg: Color) -> impl IntoElement {
    div()
        .px(px(6.))
        .rounded_sm()
        .bg(bg)
        .child(Label::new(label.into()).size(LabelSize::Small).color(fg))
}

/// Resolve the filename color for `entry` from the active theme overlay.
/// Falls back to the conservative built-in palette (directory accent /
/// hidden / default) when `FmThemeStore` is absent — that path is only
/// hit in tests and in the brief window before `theme::init` runs.
fn filetype_color(entry: &DirEntry, cx: &App) -> Color {
    if let Some(store) = cx.try_global::<FmThemeStore>() {
        return store.color_for(entry);
    }
    if entry.is_dir {
        Color::Accent
    } else if entry.is_hidden {
        Color::Hidden
    } else {
        Color::Default
    }
}

/// Status palette for one entry: leading glyph + glyph color +
/// filename tint. Filename tint is `None` when git status is clean (or
/// ignored, which dims but doesn't tint) so the caller can fall back to
/// the filetype color rather than recolor over it. Worktree changes
/// outrank index changes — that's what the user is actively editing.
fn git_status_palette(status: Option<FileStatus>) -> (&'static str, Color, Option<Color>) {
    match status {
        None => (" ", Color::Muted, None),
        // Ignored entries get no glyph but a dim filename so they read
        // as "tracked-as-not-interesting".
        Some(FileStatus::Ignored) => (" ", Color::Muted, Some(Color::Disabled)),
        // Untracked: low-contrast glyph (user hasn't told git about it
        // yet) but a clear `info` filename so the row pops in `git
        // status` parlance.
        Some(FileStatus::Untracked) => ("?", Color::Muted, Some(Color::Info)),
        // Merge conflicts use the brightest tint in the palette to
        // demand attention.
        Some(FileStatus::Unmerged(_)) => ("!", Color::Conflict, Some(Color::Conflict)),
        Some(FileStatus::Tracked(tracked)) => {
            use git::status::StatusCode::*;
            // Worktree (unstaged) wins when both sides have a change —
            // it's what the user is actively editing.
            let (code, from_worktree) = match tracked.worktree_status {
                Unmodified => (tracked.index_status, false),
                other => (other, true),
            };
            match code {
                Modified | TypeChanged => {
                    // Staged-only (no worktree change) shows the staged-
                    // bold flavor by promoting modified to created-like
                    // bold green — but the renderer doesn't have a bold
                    // variant for arbitrary colors, so the glyph picks
                    // up `Created` when the change is index-only, and
                    // `Modified` (yellow) when worktree-dirty. Filename
                    // tracks the glyph for clarity.
                    let color = if from_worktree {
                        Color::Modified
                    } else {
                        Color::Created
                    };
                    ("M", color, Some(color))
                }
                Added => ("A", Color::Created, Some(Color::Created)),
                Deleted => ("D", Color::Deleted, Some(Color::Deleted)),
                Renamed => ("R", Color::Hint, Some(Color::Hint)),
                Copied => ("C", Color::Created, Some(Color::Created)),
                Unmodified => (" ", Color::Muted, None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::status::{StatusCode, TrackedStatus, UnmergedStatus, UnmergedStatusCode};

    #[test]
    fn human_size_bytes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1), "1 B");
        assert_eq!(human_size(1023), "1023 B");
    }

    #[test]
    fn human_size_kilobytes() {
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1024 * 1023), "1023.0 KB");
    }

    #[test]
    fn human_size_megabytes() {
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
    }

    #[test]
    fn human_size_gigabytes_and_terabytes() {
        assert_eq!(human_size(1024_u64.pow(3)), "1.0 GB");
        assert_eq!(human_size(1024_u64.pow(4)), "1.0 TB");
        // Petabytes still render with the TB unit (deliberate cap).
        assert_eq!(human_size(2 * 1024_u64.pow(4)), "2.0 TB");
    }

    #[test]
    fn git_status_palette_none_has_blank_glyph_and_no_filename_tint() {
        assert_eq!(git_status_palette(None), (" ", Color::Muted, None));
    }

    #[test]
    fn git_status_palette_ignored_dims_filename() {
        let (glyph, glyph_color, filename) = git_status_palette(Some(FileStatus::Ignored));
        assert_eq!(glyph, " ");
        assert_eq!(glyph_color, Color::Muted);
        assert_eq!(filename, Some(Color::Disabled));
    }

    #[test]
    fn git_status_palette_untracked_is_info_filename() {
        let (glyph, _, filename) = git_status_palette(Some(FileStatus::Untracked));
        assert_eq!(glyph, "?");
        assert_eq!(filename, Some(Color::Info));
    }

    #[test]
    fn git_status_palette_unmerged_is_conflict_bang() {
        let status = FileStatus::Unmerged(UnmergedStatus {
            first_head: UnmergedStatusCode::Added,
            second_head: UnmergedStatusCode::Added,
        });
        let (glyph, glyph_color, filename) = git_status_palette(Some(status));
        assert_eq!(glyph, "!");
        assert_eq!(glyph_color, Color::Conflict);
        assert_eq!(filename, Some(Color::Conflict));
    }

    fn tracked(worktree: StatusCode, index: StatusCode) -> FileStatus {
        FileStatus::Tracked(TrackedStatus {
            worktree_status: worktree,
            index_status: index,
        })
    }

    #[test]
    fn git_status_palette_tracked_worktree_wins_over_index() {
        // Worktree modified, index added — worktree wins (user is
        // actively editing). Filename + glyph both Modified yellow.
        let (glyph, glyph_color, filename) =
            git_status_palette(Some(tracked(StatusCode::Modified, StatusCode::Added)));
        assert_eq!(glyph, "M");
        assert_eq!(glyph_color, Color::Modified);
        assert_eq!(filename, Some(Color::Modified));
    }

    #[test]
    fn git_status_palette_tracked_falls_back_to_index_when_worktree_unmodified() {
        let (glyph, glyph_color, filename) =
            git_status_palette(Some(tracked(StatusCode::Unmodified, StatusCode::Added)));
        assert_eq!(glyph, "A");
        assert_eq!(glyph_color, Color::Created);
        assert_eq!(filename, Some(Color::Created));
    }

    #[test]
    fn git_status_palette_tracked_staged_modified_is_created_green() {
        // Index-only Modified is "staged" — promotes the tint to
        // Created (green) so staged-vs-dirty reads at a glance.
        let (glyph, glyph_color, _) =
            git_status_palette(Some(tracked(StatusCode::Unmodified, StatusCode::Modified)));
        assert_eq!(glyph, "M");
        assert_eq!(glyph_color, Color::Created);
    }

    #[test]
    fn git_status_palette_tracked_all_codes() {
        let cases = [
            (StatusCode::Modified, "M"),
            (StatusCode::TypeChanged, "M"),
            (StatusCode::Added, "A"),
            (StatusCode::Deleted, "D"),
            (StatusCode::Renamed, "R"),
            (StatusCode::Copied, "C"),
        ];
        for (code, glyph) in cases {
            let (g, _, _) = git_status_palette(Some(tracked(code, StatusCode::Unmodified)));
            assert_eq!(g, glyph, "code {code:?} should map to {glyph}");
        }
    }

    #[test]
    fn format_permissions_known_modes() {
        assert_eq!(format_permissions(true, false, Some(0o755)), "drwxr-xr-x");
        assert_eq!(format_permissions(false, false, Some(0o644)), "-rw-r--r--");
        assert_eq!(format_permissions(false, true, Some(0o777)), "lrwxrwxrwx");
        assert_eq!(format_permissions(false, false, None), "----------");
    }

    #[test]
    fn format_owner_handles_missing_ids() {
        assert_eq!(format_owner(Some(501), Some(20)), "501:20");
        assert_eq!(format_owner(None, None), "?:?");
        assert_eq!(format_owner(Some(0), None), "0:?");
    }

    #[test]
    fn parent_fraction_holds_at_default() {
        let f = parent_fraction(crate::prefs::PREVIEW_FRACTION_DEFAULT);
        assert!((f - 0.25).abs() < 1e-4);
    }

    #[test]
    fn parent_fraction_shrinks_as_preview_grows() {
        let f_default = parent_fraction(crate::prefs::PREVIEW_FRACTION_DEFAULT);
        let f_big = parent_fraction(0.80);
        assert!(f_big < f_default);
        assert!(f_big > 0.0);
    }

    #[test]
    fn parent_fraction_clamped_below_default() {
        let f = parent_fraction(0.10);
        assert!((f - 0.25).abs() < 1e-4);
    }

    #[test]
    fn middle_column_never_collapses_at_ceiling() {
        let preview = crate::prefs::PREVIEW_FRACTION_MAX;
        let parent = parent_fraction(preview);
        let middle = 1.0 - preview - parent;
        assert!(middle > 0.10);
    }

    #[test]
    fn sort_chip_label_includes_arrow_unless_random() {
        use crate::prefs::SortMode;
        assert_eq!(sort_chip_label(SortMode::Name, false), "name ↓");
        assert_eq!(sort_chip_label(SortMode::Name, true), "name ↑");
        assert_eq!(sort_chip_label(SortMode::Mtime, false), "mtime ↓");
        assert_eq!(sort_chip_label(SortMode::Extension, true), "ext ↑");
        // Random sort is direction-agnostic — no arrow.
        assert_eq!(sort_chip_label(SortMode::Random, false), "rand");
        assert_eq!(sort_chip_label(SortMode::Random, true), "rand");
    }

    #[test]
    fn truncate_label_passthrough_short_input() {
        assert_eq!(truncate_label("abc", 20), "abc");
    }

    #[test]
    fn truncate_label_ellipsizes_long_input() {
        let truncated = truncate_label("abcdefghij", 5);
        assert_eq!(truncated.chars().count(), 5);
        assert!(truncated.ends_with('…'));
    }

    fn entry(name: &str) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: std::path::PathBuf::from(name),
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
            labels: Default::default(),
            icon_path: None,
        }
    }

    #[test]
    fn render_entry_snapshot_is_stable_until_explicitly_rebuilt() {
        let mut entries = vec![entry("first"), entry("second")];
        entries[0].size = 42;

        let snapshot = render_entries_snapshot(&entries);
        entries[0].name = "changed".into();
        entries.push(entry("third"));

        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].name, "first");
        assert_eq!(snapshot[0].size, 42);

        let shared = snapshot.clone();
        assert!(Arc::ptr_eq(&snapshot[0], &shared[0]));
    }

    #[test]
    fn marked_indices_are_listing_local_and_reappear_after_filter_clear() {
        let full = vec![entry("a"), entry("b"), entry("c")];
        let filtered = vec![entry("a")];
        let parent = vec![entry("parent-a"), entry("parent-b")];
        let marked = std::collections::BTreeSet::from([std::path::PathBuf::from("b")]);

        assert_eq!(
            marked_indices_for_entries(&full, &marked),
            std::collections::HashSet::from([1])
        );
        assert!(
            marked_indices_for_entries(&filtered, &marked).is_empty(),
            "a filtered-out mark must remain owned without highlighting another row"
        );
        assert!(
            marked_indices_for_entries(&parent, &marked).is_empty(),
            "parent indices must never collide with current-column marks"
        );
        assert_eq!(
            marked_indices_for_entries(&full, &marked),
            std::collections::HashSet::from([1]),
            "clearing the filter restores the mark"
        );
    }

    #[test]
    fn count_find_matches_is_case_insensitive_substring() {
        let entries = vec![entry("Foo.rs"), entry("bar.rs"), entry("FooBar.md")];
        assert_eq!(count_find_matches(&entries, "foo"), 2);
        assert_eq!(count_find_matches(&entries, "BAR"), 2);
        assert_eq!(count_find_matches(&entries, "missing"), 0);
        assert_eq!(count_find_matches(&entries, ""), 0);
    }

    #[test]
    fn entry_meta_label_none_mode() {
        let entry = DirEntry {
            name: "x".into(),
            path: std::path::PathBuf::from("/x"),
            is_dir: false,
            is_hidden: false,
            is_symlink: false,
            size: 100,
            git_status: None,
            mtime: None,
            btime: None,
            mode: Some(0o644),
            uid: Some(501),
            gid: Some(20),
            child_count: None,
            labels: Default::default(),
            icon_path: None,
        };
        assert_eq!(entry_meta_label(&entry, LineMode::None), None);
        assert_eq!(
            entry_meta_label(&entry, LineMode::Size).as_deref(),
            Some("100 B")
        );
        assert_eq!(
            entry_meta_label(&entry, LineMode::Permissions).as_deref(),
            Some("-rw-r--r--")
        );
        assert_eq!(
            entry_meta_label(&entry, LineMode::Owner).as_deref(),
            Some("501:20")
        );
    }

    #[test]
    fn entry_meta_label_size_mode_directory_shows_child_count() {
        let mut dir = DirEntry {
            name: "d".into(),
            path: std::path::PathBuf::from("/d"),
            is_dir: true,
            is_hidden: false,
            is_symlink: false,
            size: 0,
            git_status: None,
            mtime: None,
            btime: None,
            mode: Some(0o755),
            uid: None,
            gid: None,
            child_count: Some(3),
            labels: Default::default(),
            icon_path: None,
        };
        assert_eq!(
            entry_meta_label(&dir, LineMode::Size).as_deref(),
            Some("3 items")
        );
        dir.child_count = Some(1);
        assert_eq!(
            entry_meta_label(&dir, LineMode::Size).as_deref(),
            Some("1 item")
        );
        dir.child_count = Some(0);
        assert_eq!(
            entry_meta_label(&dir, LineMode::Size).as_deref(),
            Some("0 items")
        );
        dir.child_count = None;
        assert_eq!(entry_meta_label(&dir, LineMode::Size), None);
    }
}
