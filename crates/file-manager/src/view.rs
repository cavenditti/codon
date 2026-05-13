use git::status::FileStatus;
use gpui::{App, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px, uniform_list};
use theme::ActiveTheme;
use ui::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex,
};

use crate::file_manager::{DirEntry, FileManager, PendingInput, Preview};

impl FileManager {
    fn render_entry(
        &self,
        entry: &DirEntry,
        index: usize,
        selected: Option<usize>,
        dimmed: bool,
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
                Label::new(entry.name.clone())
                    .size(LabelSize::Small)
                    .color(text_color)
                    .single_line(),
            )
            .when(symlink_indicator, |el| {
                el.child(
                    Icon::new(IconName::ArrowUpRight)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
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

        v_flex()
            .flex_1()
            .overflow_hidden()
            .bg(bg)
            .py(px(2.))
            .children(
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| self.render_entry(entry, i, None, dimmed, cx)),
            )
    }

    fn render_preview(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.colors().surface_background;

        v_flex()
            .flex_1()
            .overflow_hidden()
            .bg(bg)
            .py(px(2.))
            .child(match &self.preview {
                Preview::Directory(entries) => div().children(
                    entries
                        .iter()
                        .enumerate()
                        .map(|(i, entry)| self.render_entry(entry, i, None, true, cx)),
                ),
                Preview::FileContent(content) => div().child(
                    div().px(px(8.)).py(px(2.)).child(
                        Label::new(content.clone())
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                ),
                Preview::Binary => div().child(
                    div().px(px(8.)).child(
                        Label::new("[binary]")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                ),
                Preview::Empty => div().child(
                    div().px(px(8.)).child(
                        Label::new("[empty]")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                ),
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
            if marked_count > 0 {
                parts.push(format!("{marked_count} marked"));
            }
            parts.join(" | ")
        };
        let error_message = self.error_message.clone();

        let entries = self.entries.clone();
        let marked = self.marked.clone();
        let this = cx.entity().downgrade();
        let focus = self.focus_handle.clone();

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
                                        Label::new(entry.name.clone())
                                            .size(LabelSize::Small)
                                            .color(text_color)
                                            .single_line(),
                                    )
                                    .when(entry.is_symlink, |el| {
                                        el.child(
                                            Icon::new(IconName::ArrowUpRight)
                                                .size(IconSize::XSmall)
                                                .color(Color::Muted),
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
                            .w_1_4()
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
                            .w_1_3()
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
}
