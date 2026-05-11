use codon_mode::{CodonModeTracker, ObjectKind, PaneMode, Selection, SelectionSource};
use gpui::{
    actions, div, prelude::*, px, uniform_list, App, ClipboardItem, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, KeyContext, Render, ScrollStrategy, SharedString, Styled,
    Task, UniformListScrollHandle, WeakEntity, Window,
};
use std::cmp;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{h_flex, v_flex, Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, StyledExt};
use util::ResultExt;
use workspace::{Item, item::ItemEvent, Workspace};

actions!(
    file_manager,
    [
        Open,
        NavigateUp,
        NavigateDown,
        EnterDirectory,
        ParentDirectory,
        GoToTop,
        GoToBottom,
        ToggleHidden,
        CreateFile,
        CreateDirectory,
        DeleteEntry,
        RenameEntry,
        YankPath,
        ToggleMark,
        CopyMarked,
        MoveMarked,
    ]
);

#[derive(Clone)]
struct DirEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_hidden: bool,
    is_symlink: bool,
    size: u64,
}

#[derive(Clone)]
enum Preview {
    Directory(Vec<DirEntry>),
    FileContent(String),
    Binary,
    Empty,
}

pub struct FileManager {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    mode: PaneMode,
    current_dir: PathBuf,
    entries: Vec<DirEntry>,
    selected_index: usize,
    marked: BTreeSet<usize>,
    parent_entries: Vec<DirEntry>,
    preview: Preview,
    show_hidden: bool,
    fs: Arc<dyn fs::Fs>,
    scroll_handle: UniformListScrollHandle,
    visible_lines: usize,
    pending_input: Option<PendingInput>,
}

#[derive(Clone)]
enum PendingInput {
    CreateFile(String),
    CreateDirectory(String),
    Rename { original: PathBuf, new_name: String },
}

#[derive(Clone, Debug)]
pub enum FileManagerEvent {
    PathChanged,
}

impl EventEmitter<FileManagerEvent> for FileManager {}

impl FileManager {
    pub fn new(
        initial_dir: PathBuf,
        workspace: WeakEntity<Workspace>,
        fs: Arc<dyn fs::Fs>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |this: &mut Self, _window, cx| {
            let tracker = cx.global_mut::<CodonModeTracker>();
            tracker.mode = this.mode;
            tracker.detail = None;
        })
        .detach();

        let mut this = Self {
            focus_handle,
            workspace,
            mode: PaneMode::Normal,
            current_dir: initial_dir,
            entries: Vec::new(),
            selected_index: 0,
            marked: BTreeSet::new(),
            parent_entries: Vec::new(),
            preview: Preview::Empty,
            show_hidden: false,
            fs,
            scroll_handle: UniformListScrollHandle::new(),
            visible_lines: 30,
            pending_input: None,
        };
        this.reload_entries_sync();
        this
    }

    fn dispatch_context(&self) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add("FileManager");
        match self.mode {
            PaneMode::Normal => context.set("pane_mode", "normal"),
            PaneMode::Insert => context.set("pane_mode", "insert"),
            PaneMode::Command => context.set("pane_mode", "command"),
        }
        context
    }

    fn reload_entries_sync(&mut self) {
        self.entries = read_dir_sync(&self.current_dir, self.show_hidden);
        self.parent_entries = self
            .current_dir
            .parent()
            .map(|p| read_dir_sync(p, self.show_hidden))
            .unwrap_or_default();
        self.selected_index = cmp::min(
            self.selected_index,
            self.entries.len().saturating_sub(1),
        );
        self.marked.clear();
        self.update_preview_sync();
    }

    fn reload_entries(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_entries_sync();
        self.ensure_visible();
        cx.emit(FileManagerEvent::PathChanged);
        cx.notify();
    }

    fn update_preview_sync(&mut self) {
        let Some(entry) = self.entries.get(self.selected_index) else {
            self.preview = Preview::Empty;
            return;
        };

        if entry.is_dir {
            let children = read_dir_sync(&entry.path, self.show_hidden);
            self.preview = Preview::Directory(children);
        } else {
            match std::fs::read_to_string(&entry.path) {
                Ok(content) => {
                    let truncated: String = content.lines().take(80).collect::<Vec<_>>().join("\n");
                    self.preview = Preview::FileContent(truncated);
                }
                Err(_) => {
                    self.preview = Preview::Binary;
                }
            }
        }
    }

    fn ensure_visible(&self) {
        // Only scroll when the selected item would be off-screen.
        // Non-strict: does nothing if already visible.
        self.scroll_handle
            .scroll_to_item(self.selected_index, ScrollStrategy::Center);
    }

    fn navigate_down(&mut self, _: &NavigateDown, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = cmp::min(self.selected_index + 1, self.entries.len() - 1);
            self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Bottom);
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn navigate_up(&mut self, _: &NavigateUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = self.selected_index.saturating_sub(1);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.update_preview_sync();
        cx.notify();
    }

    fn half_page_down(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            let half = self.visible_lines / 2;
            self.selected_index = cmp::min(self.selected_index + half, self.entries.len() - 1);
            self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Bottom);
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn half_page_up(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let half = self.visible_lines / 2;
        self.selected_index = self.selected_index.saturating_sub(half);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.update_preview_sync();
        cx.notify();
    }

    fn page_down(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = cmp::min(
                self.selected_index + self.visible_lines,
                self.entries.len() - 1,
            );
            self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Bottom);
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn page_up(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = self.selected_index.saturating_sub(self.visible_lines);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.update_preview_sync();
        cx.notify();
    }

    fn enter_directory(
        &mut self,
        _: &EnterDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.entries.get(self.selected_index).cloned() else {
            return;
        };

        if entry.is_dir {
            self.current_dir = entry.path;
            self.selected_index = 0;
            self.reload_entries(window, cx);
        } else {
            let path = entry.path.clone();
            let workspace = self.workspace.clone();
            cx.spawn_in(window, async move |_, cx| {
                if let Ok(task) = workspace.update_in(cx, |workspace, window, cx| {
                    workspace.open_abs_path(path, Default::default(), window, cx)
                }) {
                    task.await.log_err();
                }
            })
            .detach();
        }
    }

    fn parent_directory(
        &mut self,
        _: &ParentDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(parent) = self.current_dir.parent() {
            let old_dir_name = self
                .current_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string());
            self.current_dir = parent.to_path_buf();
            self.selected_index = 0;
            self.reload_entries(window, cx);

            if let Some(name) = old_dir_name {
                if let Some(idx) = self.entries.iter().position(|e| e.name == name) {
                    self.selected_index = idx;
                    self.update_preview_sync();
                    cx.notify();
                }
            }
        }
    }

    fn go_to_top(&mut self, _: &GoToTop, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = 0;
        self.ensure_visible();
        self.update_preview_sync();
        cx.notify();
    }

    fn go_to_bottom(&mut self, _: &GoToBottom, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
            self.ensure_visible();
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn toggle_hidden(&mut self, _: &ToggleHidden, window: &mut Window, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.reload_entries(window, cx);
    }

    fn toggle_mark(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked.contains(&self.selected_index) {
            self.marked.remove(&self.selected_index);
        } else {
            self.marked.insert(self.selected_index);
        }
        // Move down after marking (yazi behavior)
        if self.selected_index < self.entries.len().saturating_sub(1) {
            self.selected_index += 1;
            self.update_preview_sync();
        }
        cx.notify();
    }

    fn yank_path(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(self.selected_index) {
            let path_str = entry.path.display().to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(path_str));
        }
    }

    fn create_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::CreateFile(String::new()));
        cx.notify();
    }

    fn create_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::CreateDirectory(String::new()));
        cx.notify();
    }

    fn rename_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(self.selected_index) {
            self.mode = PaneMode::Insert;
            self.pending_input = Some(PendingInput::Rename {
                original: entry.path.clone(),
                new_name: entry.name.clone(),
            });
            cx.notify();
        }
    }

    fn delete_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let targets: Vec<PathBuf> = if self.marked.is_empty() {
            self.entries
                .get(self.selected_index)
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            self.marked
                .iter()
                .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
                .collect()
        };

        for path in &targets {
            if path.is_dir() {
                std::fs::remove_dir_all(path).log_err();
            } else {
                std::fs::remove_file(path).log_err();
            }
        }
        self.reload_entries(window, cx);
    }

    fn handle_insert_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let Some(pending) = &mut self.pending_input else {
            return;
        };

        match key {
            "escape" => {
                self.pending_input = None;
                self.mode = PaneMode::Normal;
                cx.notify();
            }
            "backspace" => {
                match pending {
                    PendingInput::CreateFile(s)
                    | PendingInput::CreateDirectory(s)
                    | PendingInput::Rename { new_name: s, .. } => {
                        s.pop();
                    }
                }
                cx.notify();
            }
            "enter" | "\n" => {
                let pending = self.pending_input.take().unwrap();
                match pending {
                    PendingInput::CreateFile(name) if !name.is_empty() => {
                        let path = self.current_dir.join(&name);
                        std::fs::write(&path, "").log_err();
                    }
                    PendingInput::CreateDirectory(name) if !name.is_empty() => {
                        let path = self.current_dir.join(&name);
                        std::fs::create_dir_all(&path).log_err();
                    }
                    PendingInput::Rename { original, new_name } if !new_name.is_empty() => {
                        let new_path = original.parent().unwrap_or(Path::new("/")).join(&new_name);
                        std::fs::rename(&original, &new_path).log_err();
                    }
                    _ => {}
                }
                self.mode = PaneMode::Normal;
                self.reload_entries(window, cx);
            }
            _ => {
                if let Some(ch) = event.keystroke.key_char.as_deref().or(Some(key)) {
                    if ch.len() == 1 {
                        match pending {
                            PendingInput::CreateFile(s)
                            | PendingInput::CreateDirectory(s)
                            | PendingInput::Rename { new_name: s, .. } => {
                                s.push_str(ch);
                            }
                        }
                        cx.notify();
                    }
                }
            }
        }
        cx.stop_propagation();
    }

    fn handle_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == PaneMode::Insert {
            self.handle_insert_key(event, window, cx);
            return;
        }
        if self.mode != PaneMode::Normal {
            return;
        }
        let key = event.keystroke.key.as_str();
        let shift = event.keystroke.modifiers.shift;
        let ctrl = event.keystroke.modifiers.control;
        let handled = match key {
            // Navigation
            "j" if !shift && !ctrl => { self.navigate_down(&NavigateDown, window, cx); true }
            "k" if !shift && !ctrl => { self.navigate_up(&NavigateUp, window, cx); true }
            "l" if !shift && !ctrl => { self.enter_directory(&EnterDirectory, window, cx); true }
            "enter" | "\n" => { self.enter_directory(&EnterDirectory, window, cx); true }
            "h" if !shift && !ctrl => { self.parent_directory(&ParentDirectory, window, cx); true }
            "g" if shift => { self.go_to_bottom(&GoToBottom, window, cx); true }
            "g" if !shift => { self.go_to_top(&GoToTop, window, cx); true }
            // Scrolling
            "d" if ctrl => { self.half_page_down(window, cx); true }
            "u" if ctrl => { self.half_page_up(window, cx); true }
            "pagedown" => { self.page_down(window, cx); true }
            "pageup" => { self.page_up(window, cx); true }
            // Selection
            "v" if !shift && !ctrl => { self.toggle_mark(window, cx); true }
            // File operations
            "y" if !shift => { self.yank_path(window, cx); true }
            "a" if !shift => { self.create_file(window, cx); true }
            "a" if shift => { self.create_directory(window, cx); true }
            "d" if !shift && !ctrl => { self.delete_entry(window, cx); true }
            "r" if !shift => { self.rename_entry(window, cx); true }
            // Toggles
            "." => { self.toggle_hidden(&ToggleHidden, window, cx); true }
            // Command mode
            ";" if shift => {
                window.dispatch_action(Box::new(zed_actions::command_palette::Toggle), cx);
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn render_entry(
        &self,
        entry: &DirEntry,
        index: usize,
        selected: Option<usize>,
        dimmed: bool,
        cx: &App,
    ) -> impl IntoElement {
        let is_selected = selected == Some(index);
        let is_marked = self.marked.contains(&index);
        let theme = cx.theme();
        let selected_bg = theme.colors().ghost_element_selected;

        let text_color = if is_marked {
            Color::Accent
        } else if dimmed {
            Color::Muted
        } else if entry.is_dir {
            Color::Accent
        } else {
            Color::Default
        };

        // File icon from Zed's icon system
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
        let marked_bg = theme.colors().ghost_element_hover;

        h_flex()
            .w_full()
            .px(px(4.))
            .py(px(1.))
            .gap(px(4.))
            .when(is_marked && !is_selected, |d| d.bg(marked_bg))
            .when(is_selected, |d| d.bg(selected_bg))
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
            .children(entries.iter().enumerate().map(|(i, entry)| {
                self.render_entry(entry, i, None, dimmed, cx)
            }))
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
                    entries.iter().enumerate().map(|(i, entry)| {
                        self.render_entry(entry, i, None, true, cx)
                    }),
                ),
                Preview::FileContent(content) => div().child(
                    div()
                        .px(px(8.))
                        .py(px(2.))
                        .child(
                            Label::new(content.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                ),
                Preview::Binary => div().child(
                    div().px(px(8.)).child(
                        Label::new("[binary]").size(LabelSize::Small).color(Color::Muted),
                    ),
                ),
                Preview::Empty => div().child(
                    div().px(px(8.)).child(
                        Label::new("[empty]").size(LabelSize::Small).color(Color::Muted),
                    ),
                ),
            })
    }

    fn render_input_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let Some(pending) = &self.pending_input else {
            return div().into_any_element();
        };

        let (label, value) = match pending {
            PendingInput::CreateFile(s) => ("new file: ", s.as_str()),
            PendingInput::CreateDirectory(s) => ("new dir: ", s.as_str()),
            PendingInput::Rename { new_name, .. } => ("rename: ", new_name.as_str()),
        };

        let theme = cx.theme();

        h_flex()
            .px(px(8.))
            .py(px(2.))
            .bg(theme.colors().editor_background)
            .border_t_1()
            .border_color(theme.colors().border)
            .child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
            .child(Label::new(format!("{value}▏")).size(LabelSize::Small).color(Color::Default))
            .into_any_element()
    }
}

impl Focusable for FileManager {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
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

        let status_text = if marked_count > 0 {
            format!(
                "{} | {}/{} | {} marked",
                dir_display,
                selected_index + 1,
                entry_count,
                marked_count
            )
        } else {
            format!(
                "{} | {}/{}",
                dir_display,
                if entry_count > 0 { selected_index + 1 } else { 0 },
                entry_count
            )
        };

        // Clone entries for the uniform_list closure
        let entries = self.entries.clone();
        let marked = self.marked.clone();
        let this = cx.entity().downgrade();
        let focus = self.focus_handle.clone();

        let current_col = uniform_list("file-list", entries.len(), {
            move |range, window, cx| {
                let theme = cx.theme();
                let selected_bg = theme.colors().ghost_element_selected;

                range
                    .map(|i| {
                        let entry = &entries[i];
                        let is_selected = i == selected_index;
                        let is_marked = marked.contains(&i);

                        let text_color = if is_marked {
                            Color::Accent
                        } else if entry.is_dir {
                            Color::Accent
                        } else {
                            Color::Default
                        };

                        let icon_element = if entry.is_dir {
                            match file_icons::FileIcons::get_folder_icon(false, &entry.path, cx) {
                                Some(p) => Icon::from_path(p).size(IconSize::Small).color(Color::Muted).into_any_element(),
                                None => Icon::new(IconName::Folder).size(IconSize::Small).color(Color::Muted).into_any_element(),
                            }
                        } else {
                            match file_icons::FileIcons::get_icon(&entry.path, cx) {
                                Some(p) => Icon::from_path(p).size(IconSize::Small).color(Color::Muted).into_any_element(),
                                None => Icon::new(IconName::File).size(IconSize::Small).color(Color::Muted).into_any_element(),
                            }
                        };

                        let marked_bg = theme.colors().ghost_element_hover;
                        let this = this.clone();
                        let focus = focus.clone();

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
                                    .child(icon_element)
                                    .child(Label::new(entry.name.clone()).size(LabelSize::Small).color(text_color).single_line())
                                    .when(entry.is_symlink, |el| {
                                        el.child(Icon::new(IconName::ArrowUpRight).size(IconSize::XSmall).color(Color::Muted))
                                    }),
                            )
                            .on_click(move |_event, window, cx| {
                                focus.focus(window, cx);
                                this.update(cx, |fm, cx| {
                                    fm.selected_index = i;
                                    fm.update_preview_sync();
                                    cx.notify();
                                }).ok();
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
            .on_key_down(cx.listener(Self::handle_key_down))
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
            .child(
                div()
                    .px(px(8.))
                    .py(px(1.))
                    .border_t_1()
                    .border_color(border_color)
                    .child(Label::new(status_text).size(LabelSize::Small).color(Color::Muted)),
            )
    }
}

impl Item for FileManager {
    type Event = FileManagerEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        self.current_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string())
            .into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Folder))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            FileManagerEvent::PathChanged => f(ItemEvent::UpdateTab),
        }
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<workspace::WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>> {
        let dir = self.current_dir.clone();
        let workspace = self.workspace.clone();
        let fs = self.fs.clone();
        Task::ready(Some(cx.new(|cx| Self::new(dir, workspace, fs, window, cx))))
    }

    fn can_split(&self) -> bool {
        true
    }
}

impl SelectionSource for FileManager {
    fn current_selection(&self) -> Selection {
        if !self.marked.is_empty() {
            let paths: Vec<PathBuf> = self
                .marked
                .iter()
                .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
                .collect();
            Selection::Files(paths)
        } else {
            match self.entries.get(self.selected_index) {
                Some(entry) => Selection::Files(vec![entry.path.clone()]),
                None => Selection::Empty,
            }
        }
    }

    fn object_kinds(&self) -> &'static [ObjectKind] {
        &[ObjectKind::File, ObjectKind::Dir]
    }
}

fn read_dir_sync(path: &Path, show_hidden: bool) -> Vec<DirEntry> {
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return Vec::new();
    };

    let mut entries: Vec<DirEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_hidden = name.starts_with('.');
            if !show_hidden && is_hidden {
                return None;
            }
            let metadata = e.metadata().ok()?;
            let file_type = e.file_type().ok()?;
            Some(DirEntry {
                name,
                path: e.path(),
                is_dir: metadata.is_dir(),
                is_hidden,
                is_symlink: file_type.is_symlink(),
                size: metadata.len(),
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries
}

pub fn init(cx: &mut App) {
    let registry = cx.global_mut::<codon_mode::ActionAcceptsRegistry>();
    registry.register::<NavigateUp>(&[ObjectKind::File, ObjectKind::Dir]);
    registry.register::<NavigateDown>(&[ObjectKind::File, ObjectKind::Dir]);
    registry.register::<EnterDirectory>(&[ObjectKind::File, ObjectKind::Dir]);
    registry.register::<ParentDirectory>(&[ObjectKind::Dir]);
    registry.register::<ToggleHidden>(&[]);
    registry.register::<Open>(&[]);
    registry.register::<CreateFile>(&[]);
    registry.register::<CreateDirectory>(&[]);
    registry.register::<DeleteEntry>(&[ObjectKind::File, ObjectKind::Dir]);
    registry.register::<RenameEntry>(&[ObjectKind::File, ObjectKind::Dir]);
    registry.register::<YankPath>(&[ObjectKind::File, ObjectKind::Dir]);
    registry.register::<ToggleMark>(&[ObjectKind::File, ObjectKind::Dir]);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &Open, window, cx| {
            open_file_manager(workspace, window, cx);
        });
        workspace.register_action(
            |workspace, _: &zed_actions::file_manager::OpenFileManager, window, cx| {
                open_file_manager(workspace, window, cx);
            },
        );
    })
    .detach();
}

fn open_file_manager(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let fs = workspace.app_state().fs.clone();
    let weak_workspace = workspace.weak_handle();
    let dir = workspace
        .project()
        .read(cx)
        .worktrees(cx)
        .next()
        .map(|wt| wt.read(cx).abs_path().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    let file_manager = cx.new(|cx| FileManager::new(dir, weak_workspace, fs, window, cx));
    workspace.add_item_to_active_pane(Box::new(file_manager), None, true, window, cx);
}
