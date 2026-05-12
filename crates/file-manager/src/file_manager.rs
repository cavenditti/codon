use codon_mode::{CodonModeTracker, ObjectKind, PaneMode, Selection, SelectionSource};
use git::status::FileStatus;
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
use ui::{h_flex, v_flex, Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize};
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
    git_status: Option<FileStatus>,
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
    filter_query: String,
    entries_unfiltered: Option<Vec<DirEntry>>,
    error_message: Option<String>,
    error_gen: u64,
}

#[derive(Clone)]
enum PendingInput {
    CreateFile(String),
    CreateDirectory(String),
    Rename { original: PathBuf, new_name: String },
    Filter,
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
            this.populate_git_status(cx);
            cx.notify();
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
            filter_query: String::new(),
            entries_unfiltered: None,
            error_message: None,
            error_gen: 0,
        };
        this.reload_entries_sync();
        this
    }

    fn surface_error(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        let msg = msg.into();
        log::warn!("file-manager: {msg}");
        self.error_gen = self.error_gen.wrapping_add(1);
        let current_gen = self.error_gen;
        self.error_message = Some(msg);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            this.update(cx, |this, cx| {
                if this.error_gen == current_gen {
                    this.error_message = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
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
        // A fresh directory listing invalidates any active filter — the
        // user navigated, so the original set is gone.
        self.filter_query.clear();
        self.entries_unfiltered = None;
        self.update_preview_sync();
    }

    fn reload_entries(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_entries_sync();
        self.populate_git_status(cx);
        self.ensure_visible();
        cx.emit(FileManagerEvent::PathChanged);
        cx.notify();
    }

    fn populate_git_status(&mut self, cx: &App) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let workspace = workspace.read(cx);
        let project = workspace.project().read(cx);
        let git_store = project.git_store().read(cx);

        let lookup = |abs_path: &Path| -> Option<FileStatus> {
            if let Some(pp) = project.project_path_for_absolute_path(abs_path, cx)
                && let Some(status) = git_store.project_path_git_status(&pp, cx)
            {
                return Some(status);
            }
            for repo in git_store.repositories().values() {
                let repo = repo.read(cx);
                if let Some(repo_path) = repo.abs_path_to_repo_path(abs_path)
                    && let Some(entry) = repo.status_for_path(&repo_path)
                {
                    return Some(entry.status);
                }
            }
            None
        };

        for entry in &mut self.entries {
            entry.git_status = lookup(&entry.path);
        }
        for entry in &mut self.parent_entries {
            entry.git_status = lookup(&entry.path);
        }
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
            let path = entry.path;
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

    fn create_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::CreateFile(String::new()));
        cx.notify();
    }

    fn create_directory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::CreateDirectory(String::new()));
        cx.notify();
    }

    fn rename_entry(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(self.selected_index) {
            self.mode = PaneMode::Insert;
            self.pending_input = Some(PendingInput::Rename {
                original: entry.path.clone(),
                new_name: entry.name.clone(),
            });
            cx.notify();
        }
    }

    fn handle_cancel(
        &mut self,
        _: &menu::Cancel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let in_filter_mode = matches!(self.pending_input, Some(PendingInput::Filter));
        let has_filter = !self.filter_query.is_empty() || self.entries_unfiltered.is_some();
        if !in_filter_mode && !has_filter {
            return;
        }
        if in_filter_mode {
            self.pending_input = None;
            self.mode = PaneMode::Normal;
        }
        self.clear_filter();
        cx.notify();
        cx.stop_propagation();
    }

    fn start_filter(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Re-entering filter mode while a filter is already committed
        // keeps the existing query so the user can edit it.
        if self.entries_unfiltered.is_none() {
            self.entries_unfiltered = Some(self.entries.clone());
            self.filter_query.clear();
        }
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::Filter);
        self.apply_filter();
        cx.notify();
    }

    fn apply_filter(&mut self) {
        let Some(unfiltered) = self.entries_unfiltered.clone() else {
            return;
        };
        if self.filter_query.is_empty() {
            self.entries = unfiltered;
            self.selected_index = 0;
            self.update_preview_sync();
            return;
        }
        let needle: Vec<char> = self
            .filter_query
            .chars()
            .map(|c| c.to_ascii_lowercase())
            .collect();
        self.entries = unfiltered
            .into_iter()
            .filter(|entry| is_subsequence(&needle, &entry.name))
            .collect();
        self.selected_index = 0;
        self.marked.clear();
        self.update_preview_sync();
    }

    fn clear_filter(&mut self) {
        self.filter_query.clear();
        if let Some(unfiltered) = self.entries_unfiltered.take() {
            self.entries = unfiltered;
        }
        self.selected_index = cmp::min(
            self.selected_index,
            self.entries.len().saturating_sub(1),
        );
        self.update_preview_sync();
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

        if targets.is_empty() {
            return;
        }

        let fs = self.fs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
            for path in targets {
                let options = fs::RemoveOptions {
                    recursive: true,
                    ignore_if_not_exists: false,
                };
                if let Err(e) = fs.trash(&path, options).await {
                    failures.push((path, e));
                }
            }
            this.update_in(cx, |this, window, cx| {
                for (path, e) in &failures {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    this.surface_error(format!("Couldn't trash {name}: {e}"), cx);
                }
                this.reload_entries(window, cx);
            })
            .ok();
        })
        .detach();
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
                let was_filter = matches!(pending, PendingInput::Filter);
                self.pending_input = None;
                self.mode = PaneMode::Normal;
                if was_filter {
                    self.clear_filter();
                }
                cx.notify();
            }
            "backspace" => {
                match pending {
                    PendingInput::CreateFile(s)
                    | PendingInput::CreateDirectory(s)
                    | PendingInput::Rename { new_name: s, .. } => {
                        s.pop();
                    }
                    PendingInput::Filter => {
                        self.filter_query.pop();
                        self.apply_filter();
                    }
                }
                cx.notify();
            }
            "enter" | "\n" => {
                let pending = self.pending_input.take().unwrap();
                match pending {
                    PendingInput::CreateFile(name) if !name.is_empty() => {
                        let path = self.current_dir.join(&name);
                        let fs = self.fs.clone();
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        cx.spawn_in(window, async move |this, cx| {
                            let result = fs
                                .create_file(&path, fs::CreateOptions::default())
                                .await;
                            this.update_in(cx, |this, window, cx| {
                                if let Err(e) = result {
                                    this.surface_error(
                                        format!("Couldn't create file {name}: {e}"),
                                        cx,
                                    );
                                }
                                this.reload_entries(window, cx);
                            })
                            .ok();
                        })
                        .detach();
                    }
                    PendingInput::CreateDirectory(name) if !name.is_empty() => {
                        let path = self.current_dir.join(&name);
                        let fs = self.fs.clone();
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        cx.spawn_in(window, async move |this, cx| {
                            let result = fs.create_dir(&path).await;
                            this.update_in(cx, |this, window, cx| {
                                if let Err(e) = result {
                                    this.surface_error(
                                        format!("Couldn't create directory {name}: {e}"),
                                        cx,
                                    );
                                }
                                this.reload_entries(window, cx);
                            })
                            .ok();
                        })
                        .detach();
                    }
                    PendingInput::Rename { original, new_name } if !new_name.is_empty() => {
                        let new_path = original.parent().unwrap_or(Path::new("/")).join(&new_name);
                        let fs = self.fs.clone();
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        cx.spawn_in(window, async move |this, cx| {
                            let result = fs
                                .rename(&original, &new_path, fs::RenameOptions::default())
                                .await;
                            this.update_in(cx, |this, window, cx| {
                                if let Err(e) = result {
                                    this.surface_error(
                                        format!("Couldn't rename to {new_name}: {e}"),
                                        cx,
                                    );
                                }
                                this.reload_entries(window, cx);
                            })
                            .ok();
                        })
                        .detach();
                    }
                    PendingInput::Filter => {
                        // Commit the filter view: leave Insert mode but keep
                        // entries narrowed. `Esc` from Normal (via clear_filter)
                        // or any reload restores the full set.
                        self.mode = PaneMode::Normal;
                        cx.notify();
                    }
                    _ => {
                        self.mode = PaneMode::Normal;
                        self.reload_entries(window, cx);
                    }
                }
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
                            PendingInput::Filter => {
                                self.filter_query.push_str(ch);
                                self.apply_filter();
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
            // Fuzzy filter
            "/" => { self.start_filter(window, cx); true }
            "escape" if !self.filter_query.is_empty() || self.entries_unfiltered.is_some() => {
                self.clear_filter();
                cx.notify();
                true
            }
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
            PendingInput::Filter => ("filter: ", self.filter_query.as_str()),
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

        let filter_active = !self.filter_query.is_empty();
        let filter_committed = filter_active && !matches!(self.pending_input, Some(PendingInput::Filter));
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

        // Clone entries for the uniform_list closure
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
                    .child(Label::new(status_text).size(LabelSize::Small).color(Color::Muted)),
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

fn is_subsequence(needle: &[char], haystack: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut idx = 0;
    for c in haystack.chars() {
        if needle[idx] == c.to_ascii_lowercase() {
            idx += 1;
            if idx == needle.len() {
                return true;
            }
        }
    }
    false
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
                git_status: None,
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
    let project = workspace.project().clone();
    let dir = project
        .read(cx)
        .worktrees(cx)
        .next()
        .map(|wt| wt.read(cx).abs_path().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    // Make sure the project has a worktree covering this directory so
    // git_store can discover the enclosing repository and per-entry
    // status lookups resolve to a real `FileStatus`.
    let needs_worktree = project
        .read(cx)
        .worktree_store()
        .read(cx)
        .find_worktree(&dir, cx)
        .is_none();
    if needs_worktree {
        let dir_arc: Arc<Path> = Arc::from(dir.as_path());
        project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(dir_arc, false, cx)
            })
            .detach_and_log_err(cx);
    }

    let file_manager = cx.new(|cx| FileManager::new(dir, weak_workspace, fs, window, cx));
    workspace.add_item_to_active_pane(Box::new(file_manager), None, true, window, cx);
}
