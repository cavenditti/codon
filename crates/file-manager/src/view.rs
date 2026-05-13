use git::status::FileStatus;
use gpui::{
    App, Context, IntoElement, ObjectFit, Render, SharedString, StyledImage, Window, div, img,
    prelude::*, px, relative, uniform_list,
};
use theme::ActiveTheme;
use ui::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex,
};

use crate::file_manager::{
    ArchiveListing, BinaryInfo, DirEntry, FileManager, ImageInfo, PendingInput, Preview,
    format_hex_dump,
};
use crate::prefs::LineMode;
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

        let text_color = if entry.is_hidden {
            Color::Hidden
        } else if dimmed {
            Color::Muted
        } else if entry.is_dir {
            Color::Accent
        } else {
            Color::Default
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
        let (git_glyph, git_color) = git_status_decoration(entry.git_status);

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
        let theme = cx.theme();
        let bg = theme.colors().surface_background;
        let line_mode = self.line_mode;

        v_flex()
            .flex_1()
            .overflow_hidden()
            .bg(bg)
            .py(px(2.))
            .children(
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| self.render_entry(entry, i, None, dimmed, line_mode, cx)),
            )
    }

    fn render_preview(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.colors().surface_background;
        let line_mode = self.line_mode;

        v_flex()
            .flex_1()
            .overflow_hidden()
            .bg(bg)
            .py(px(2.))
            .child(match &self.preview {
                Preview::Directory(entries) => div()
                    .children(
                        entries
                            .iter()
                            .enumerate()
                            .map(|(i, entry)| self.render_entry(entry, i, None, true, line_mode, cx)),
                    )
                    .into_any_element(),
                Preview::FileContent(content) => div()
                    .child(
                        div().px(px(8.)).py(px(2.)).child(
                            Label::new(content.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .into_any_element(),
                Preview::Archive(listing) => render_archive_preview(listing).into_any_element(),
                Preview::Image(info) => render_image_preview(info).into_any_element(),
                Preview::Binary(info) => render_binary_preview(info, cx).into_any_element(),
                Preview::Empty => div()
                    .child(
                        div().px(px(8.)).child(
                            Label::new("[empty]")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .into_any_element(),
            })
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parent_col = self.render_column_static(&self.parent_entries, true, cx);
        let preview_col = self.render_preview(cx);
        let input_bar = self.render_input_bar(cx);

        let theme = cx.theme();
        let border_color = theme.colors().border;
        let bg = theme.colors().surface_background;
        let dir_display = self.current_dir.display().to_string();
        let entry_count = self.entries.len();
        let marked_count = self.marked.len();
        let selected_index = self.selected_index;

        let filter_active = !self.filter_query.is_empty();
        let filter_committed =
            filter_active && !matches!(self.pending_input, Some(PendingInput::Filter));
        let filter_query = self.filter_query.clone();
        let focused_meta = self.entries.get(self.selected_index).and_then(|e| {
            if e.is_dir {
                match &self.preview {
                    Preview::Directory(children) => Some(format!("{} items", children.len())),
                    _ => None,
                }
            } else {
                Some(human_size(e.size))
            }
        });
        let status_text = {
            let position = if entry_count > 0 {
                format!("{}/{}", selected_index + 1, entry_count)
            } else {
                format!("0/{entry_count}")
            };
            let mut parts = vec![dir_display, position];
            if let Some(meta) = focused_meta {
                parts.push(meta);
            }
            if self.visual_anchor.is_some() {
                parts.push(format!("VISUAL ({marked_count})"));
            } else if marked_count > 0 {
                parts.push(format!("{marked_count} marked"));
            }
            parts.join(" | ")
        };
        let error_message = self.error_message.clone();

        let entries = self.entries.clone();
        let marked = self.marked.clone();
        let this = cx.entity().downgrade();
        let focus = self.focus_handle.clone();
        let line_mode = self.line_mode;

        let current_col = uniform_list("file-list", entries.len(), {
            move |range, _window, cx| {
                let theme = cx.theme();
                let selected_bg = theme.colors().ghost_element_selected;

                range
                    .map(|i| {
                        let entry = &entries[i];
                        let is_selected = i == selected_index;
                        let is_marked = marked.contains(&i);

                        let text_color = if is_marked {
                            Color::Accent
                        } else if entry.is_hidden {
                            Color::Hidden
                        } else if entry.is_dir {
                            Color::Accent
                        } else {
                            Color::Default
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
                        let (git_glyph, git_color) = git_status_decoration(entry.git_status);
                        let meta = entry_meta_label(entry, line_mode);

                        div()
                            .id(("file-entry", i))
                            .child(
                                h_flex()
                                    .w_full()
                                    .px(px(4.))
                                    .py(px(1.))
                                    .gap(px(4.))
                                    .when(is_marked && !is_selected, |d| d.bg(marked_bg))
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
                            .on_click(move |_event, window, cx| {
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
        .bg(bg)
        .py(px(2.))
        .track_scroll(&self.scroll_handle);

        v_flex()
            .size_full()
            .key_context(self.dispatch_context())
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::handle_cancel))
            .on_key_down(cx.listener(Self::handle_key_down))
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
                            Label::new("(Esc to clear, / to edit)")
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
            .child(
                div()
                    .px(px(8.))
                    .py(px(1.))
                    .border_t_1()
                    .border_color(border_color)
                    .child(
                        Label::new(status_text)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
    }
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
        .child(v_flex().children(dump_lines.into_iter().map(|line| {
            Label::new(SharedString::from(line))
                .size(LabelSize::Small)
                .color(Color::Muted)
                .buffer_font(cx)
        })))
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
                None
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

fn git_status_decoration(status: Option<FileStatus>) -> (&'static str, Color) {
    match status {
        None => (" ", Color::Muted),
        Some(FileStatus::Ignored) => (" ", Color::Muted),
        Some(FileStatus::Untracked) => ("?", Color::Hint),
        Some(FileStatus::Unmerged(_)) => ("U", Color::Conflict),
        Some(FileStatus::Tracked(tracked)) => {
            use git::status::StatusCode::*;
            // Worktree (unstaged) wins when both sides have a change —
            // it's what the user is actively editing.
            let code = match tracked.worktree_status {
                Unmodified => tracked.index_status,
                other => other,
            };
            match code {
                Modified | TypeChanged => ("M", Color::Modified),
                Added => ("A", Color::Created),
                Deleted => ("D", Color::Deleted),
                Renamed => ("R", Color::Modified),
                Copied => ("C", Color::Created),
                Unmodified => (" ", Color::Muted),
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
    fn git_status_decoration_none_and_ignored_are_blank_muted() {
        assert_eq!(git_status_decoration(None), (" ", Color::Muted));
        assert_eq!(
            git_status_decoration(Some(FileStatus::Ignored)),
            (" ", Color::Muted),
        );
    }

    #[test]
    fn git_status_decoration_untracked_is_question() {
        assert_eq!(
            git_status_decoration(Some(FileStatus::Untracked)),
            ("?", Color::Hint),
        );
    }

    #[test]
    fn git_status_decoration_unmerged_is_conflict_glyph() {
        let status = FileStatus::Unmerged(UnmergedStatus {
            first_head: UnmergedStatusCode::Added,
            second_head: UnmergedStatusCode::Added,
        });
        assert_eq!(git_status_decoration(Some(status)), ("U", Color::Conflict));
    }

    fn tracked(worktree: StatusCode, index: StatusCode) -> FileStatus {
        FileStatus::Tracked(TrackedStatus {
            worktree_status: worktree,
            index_status: index,
        })
    }

    #[test]
    fn git_status_decoration_tracked_worktree_wins_over_index() {
        // Worktree modified, index added — worktree wins (the user is
        // actively editing).
        assert_eq!(
            git_status_decoration(Some(tracked(StatusCode::Modified, StatusCode::Added))),
            ("M", Color::Modified),
        );
    }

    #[test]
    fn git_status_decoration_tracked_falls_back_to_index_when_worktree_unmodified() {
        assert_eq!(
            git_status_decoration(Some(tracked(StatusCode::Unmodified, StatusCode::Added))),
            ("A", Color::Created),
        );
    }

    #[test]
    fn git_status_decoration_tracked_all_codes() {
        let cases = [
            (StatusCode::Modified, "M"),
            (StatusCode::TypeChanged, "M"),
            (StatusCode::Added, "A"),
            (StatusCode::Deleted, "D"),
            (StatusCode::Renamed, "R"),
            (StatusCode::Copied, "C"),
        ];
        for (code, glyph) in cases {
            let (g, _) = git_status_decoration(Some(tracked(code, StatusCode::Unmodified)));
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
        };
        assert_eq!(entry_meta_label(&entry, LineMode::None), None);
        assert_eq!(entry_meta_label(&entry, LineMode::Size).as_deref(), Some("100 B"));
        assert_eq!(
            entry_meta_label(&entry, LineMode::Permissions).as_deref(),
            Some("-rw-r--r--")
        );
        assert_eq!(entry_meta_label(&entry, LineMode::Owner).as_deref(), Some("501:20"));
    }
}
