use git::status::FileStatus;
use gpui::{
    AnyElement, App, Context, FontWeight, IntoElement, ObjectFit, Render, SharedString,
    StyledImage, Window, div, img, prelude::*, px, relative, uniform_list,
};
use theme::ActiveTheme;
use ui::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex,
};

use workspace::codon_jump_clickable::JumpClickableExt;

use crate::file_manager::{
    ArchiveListing, BinaryInfo, DirEntry, FileManager, ImageInfo, PendingInput, Preview,
    TextPreview, format_hex_dump,
};
use crate::prefs::LineMode;
use crate::theme::FmThemeStore;
use std::time::SystemTime;

impl FileManager {
    fn render_entry(
        &self,
        entry: &DirEntry,
        index: usize,
        selected: Option<usize>,
        dimmed: bool,
        line_mode: LineMode,
        cx: &App,
    ) -> impl IntoElement {
        // Marks are intrinsically tied to the current column's index space
        // (`self.entries`). `render_entry` is only called for the parent and
        // preview columns, where applying `self.marked` indices would
        // erroneously highlight rows that happen to share an index with a
        // marked current-column entry. The current column inlines its own
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

        let icon_element = if entry.is_dir {
            let folder_icon = file_icons::FileIcons::get_folder_icon(false, &entry.path, cx);
            match folder_icon {
                Some(icon_path) => Icon::from_path(icon_path)
                    .size(IconSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
                None => Icon::new(IconName::Folder)
                    .size(IconSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
            }
        } else {
            let file_icon = file_icons::FileIcons::get_icon(&entry.path, cx);
            match file_icon {
                Some(icon_path) => Icon::from_path(icon_path)
                    .size(IconSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
                None => Icon::new(IconName::File)
                    .size(IconSize::Small)
                    .color(Color::Muted)
                    .into_any_element(),
            }
        };

        let symlink_indicator = entry.is_symlink;
        let (git_glyph, git_color, git_filename_color) = git_status_palette(entry.git_status);
        // Git status wins over filetype for the filename tint when the
        // entry is dirty/untracked/etc; on clean entries the filetype
        // overlay (or muted-for-dimmed) carries the color.
        let text_color = git_filename_color.unwrap_or(text_color);

        let meta = entry_meta_label(entry, line_mode);

        h_flex()
            .w_full()
            .px(px(4.))
            .py(px(1.))
            .gap(px(4.))
            .when(is_selected, |d| d.bg(selected_bg))
            .child(
                div().w(px(12.)).child(
                    Label::new(SharedString::new_static(git_glyph))
                        .size(LabelSize::Small)
                        .color(git_color),
                ),
            )
            .child(icon_element)
            .child(
                div().flex_1().min_w_0().child(
                    Label::new(entry.name.clone())
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
                    div().w(px(META_COLUMN_WIDTH)).child(
                        Label::new(SharedString::from(text))
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
                )
            })
    }

    fn render_column_static(
        &self,
        entries: &[DirEntry],
        dimmed: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let line_mode = self.line_mode;

        // No explicit bg — the root container paints the panel color
        // once and the columns inherit it, so the three sub-panels
        // blend into one continuous surface (no card-vs-window contrast).
        v_flex()
            .flex_1()
            .overflow_hidden()
            .py(px(2.))
            .children(
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| self.render_entry(entry, i, None, dimmed, line_mode, cx)),
            )
    }

    fn render_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line_mode = self.line_mode;

        // Clone the preview snapshot so the subsequent `&mut self` call
        // for the text branch (`preview_editor_for`) doesn't overlap with
        // the immutable borrow that a `match &self.preview` would hold.
        // The variants are cheap to clone — `Text` carries at most
        // `TEXT_PREVIEW_MAX_BYTES`, `Directory` carries a `Vec<DirEntry>`
        // that's also the source for the rendered children.
        let snapshot = self.preview.clone();

        let body: AnyElement = match snapshot {
            Preview::Directory(entries) => div()
                .children(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(i, entry)| self.render_entry(entry, i, None, true, line_mode, cx)),
                )
                .into_any_element(),
            Preview::Text(text) => render_text_preview(self, &text, window, cx).into_any_element(),
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
        };

        v_flex()
            .flex_1()
            .overflow_hidden()
            .py(px(2.))
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
                (
                    "delete? ",
                    format!("{count} entries to trash — y/N").into(),
                )
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
        // `into_any_element()` is applied eagerly because Rust 2024's
        // capture rules treat `impl IntoElement` returned by `&self` /
        // `&mut self` methods as borrowing `*self` for the element's
        // entire lifetime, which would otherwise conflict with the
        // `&mut self` borrow needed by `render_preview`.
        let parent_col = self
            .render_column_static(&self.parent_entries, true, cx)
            .into_any_element();
        let preview_col = self.render_preview(window, cx);
        let input_bar = self.render_input_bar(cx).into_any_element();

        let theme = cx.theme();
        let border_color = theme.colors().border;
        // Single unified background for the whole panel — columns,
        // preview, and chrome all paint on the same surface so the FM
        // reads as one panel, not three cards floating over the window.
        let panel_bg = theme.colors().panel_background;
        let dir_display = self.current_dir.display().to_string();
        let entry_count = self.entries.len();
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
        let marked_total_size: u64 = self
            .marked
            .iter()
            .filter_map(|i| self.entries.get(*i))
            .filter(|e| !e.is_dir)
            .map(|e| e.size)
            .sum();
        let listing_total_size: u64 = self
            .entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.size)
            .sum();
        let bottom_bar_state = BottomBarState {
            entry: focused_entry.clone(),
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
        let shell_banner = self
            .shell_running
            .as_ref()
            .map(|r| r.command.clone());

        // Header-chip inputs — sourced from existing panel state. Match
        // counts are computed here once so the chip doesn't pay for an
        // O(n) scan on every cell render.
        let find_pending = self
            .pending_input
            .as_ref()
            .and_then(|p| match p {
                PendingInput::FindForward { query, .. }
                | PendingInput::FindBackward { query, .. } => Some(query.clone()),
                _ => None,
            });
        let find_active_pattern = find_pending.clone().or_else(|| self.last_find_pattern.clone());
        let find_match_count = find_active_pattern
            .as_ref()
            .map(|needle| count_find_matches(&self.entries, needle))
            .unwrap_or(0);
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

        let entries = self.entries.clone();
        let marked = self.marked.clone();
        let this = cx.entity().downgrade();
        let focus = self.focus_handle.clone();
        let line_mode = self.line_mode;

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

                        let icon_element = if entry.is_dir {
                            match file_icons::FileIcons::get_folder_icon(false, &entry.path, cx) {
                                Some(p) => Icon::from_path(p)
                                    .size(IconSize::Small)
                                    .color(Color::Muted)
                                    .into_any_element(),
                                None => Icon::new(IconName::Folder)
                                    .size(IconSize::Small)
                                    .color(Color::Muted)
                                    .into_any_element(),
                            }
                        } else {
                            match file_icons::FileIcons::get_icon(&entry.path, cx) {
                                Some(p) => Icon::from_path(p)
                                    .size(IconSize::Small)
                                    .color(Color::Muted)
                                    .into_any_element(),
                                None => Icon::new(IconName::File)
                                    .size(IconSize::Small)
                                    .color(Color::Muted)
                                    .into_any_element(),
                            }
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
                        let meta = entry_meta_label(entry, line_mode);

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
                                    .pr(px(4.))
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
                                        div().w(px(12.)).child(
                                            Label::new(SharedString::new_static(git_glyph))
                                                .size(LabelSize::Small)
                                                .color(git_color),
                                        ),
                                    )
                                    .child(icon_element)
                                    .child(
                                        div().flex_1().min_w_0().child(
                                            Label::new(entry.name.clone())
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
                                            div().w(px(META_COLUMN_WIDTH)).child(
                                                Label::new(SharedString::from(text))
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
                                        fm.update_preview_sync();
                                        cx.notify();
                                    })
                                    .ok();
                                }
                            })
                            .jump_target(move |window, cx| {
                                focus.focus(window, cx);
                                this.update(cx, |fm, cx| {
                                    fm.selected_index = i;
                                    fm.update_preview_sync();
                                    cx.notify();
                                })
                                .ok();
                            })
                    })
                    .collect()
            }
        })
        .size_full()
        .py(px(2.))
        .track_scroll(&self.scroll_handle);

        v_flex()
            .size_full()
            .bg(panel_bg)
            .key_context(self.dispatch_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_cancel))
            .on_action(cx.listener(Self::handle_choose_opener))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortByName, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Name, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortBySize, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Size, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortByMtime, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Mtime, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortByBtime, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Btime, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortByExtension, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Extension, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortByNatural, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Natural, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::SortByRandom, window, cx| {
                this.set_sort_mode(crate::prefs::SortMode::Random, window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::file_manager::ToggleSortReverse, window, cx| {
                this.toggle_sort_reverse(window, cx);
            }))
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .child(render_top_bar(&top_bar, cx))
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
                            Label::new("(Esc to clear, f to edit)")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                )
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w(relative(parent_fraction(self.preview_fraction)))
                            .h_full()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(border_color)
                            .child(parent_col),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .min_h_0()
                            .border_r_1()
                            .border_color(border_color)
                            .child(current_col),
                    )
                    .child(
                        div()
                            .w(relative(self.preview_fraction))
                            .h_full()
                            .overflow_hidden()
                            .child(preview_col),
                    ),
            )
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
            .child(render_bottom_bar(&bottom_bar_state, bottom_left_mode, cx))
    }
}

fn render_text_preview(
    fm: &mut FileManager,
    text: &TextPreview,
    window: &mut Window,
    cx: &mut Context<FileManager>,
) -> impl IntoElement {
    let editor = fm.preview_editor_for(text, window, cx);
    div().size_full().px(px(8.)).py(px(2.)).child(editor)
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
    v_flex().px(px(8.)).py(px(2.)).children(lines.into_iter().map(|line| {
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
            "zip" | "x-tar" | "x-7z-compressed" | "x-rar-compressed"
            | "gzip" | "x-bzip2" | "x-xz" | "x-zstd" => "Archive",
            "x-sharedlib" | "x-executable" | "x-mach-binary" | "vnd.microsoft.portable-executable"
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
        LineMode::Permissions => Some(format_permissions(entry.is_dir, entry.is_symlink, entry.mode)),
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
        BottomBarLeft::CmdShortcuts(hints) => {
            render_bottom_left_hints(&hints).into_any_element()
        }
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
pub(crate) fn contextual_help_hints(
    fm: &FileManager,
) -> Option<Vec<(&'static str, &'static str)>> {
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
            ("R", "bulk-rename"),
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
        ("f", "filter"),
        (".", "hidden"),
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

    let sort_is_default =
        matches!(state.sort, crate::prefs::SortMode::Name) && !state.reverse;
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
        chips.push(
            chip(label, status.warning_background, Color::Warning).into_any_element(),
        );
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
        chips.push(chip(".".to_string(), theme.colors().element_background, Color::Muted)
            .into_any_element());
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
        }
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
        };
        assert_eq!(entry_meta_label(&entry, LineMode::None), None);
        assert_eq!(entry_meta_label(&entry, LineMode::Size).as_deref(), Some("100 B"));
        assert_eq!(
            entry_meta_label(&entry, LineMode::Permissions).as_deref(),
            Some("-rw-r--r--")
        );
        assert_eq!(entry_meta_label(&entry, LineMode::Owner).as_deref(), Some("501:20"));
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
        };
        assert_eq!(entry_meta_label(&dir, LineMode::Size).as_deref(), Some("3 items"));
        dir.child_count = Some(1);
        assert_eq!(entry_meta_label(&dir, LineMode::Size).as_deref(), Some("1 item"));
        dir.child_count = Some(0);
        assert_eq!(entry_meta_label(&dir, LineMode::Size).as_deref(), Some("0 items"));
        dir.child_count = None;
        assert_eq!(entry_meta_label(&dir, LineMode::Size), None);
    }
}
