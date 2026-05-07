use gpui::{
    actions, div, prelude::*, px, App, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, Render, SharedString, Styled, Task, Window,
};
use std::cmp;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{h_flex, v_flex, Color, Icon, IconName, Label, LabelCommon, StyledExt};
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
    ]
);

#[derive(Clone)]
struct DirEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_hidden: bool,
    is_symlink: bool,
}

#[derive(Clone)]
enum Preview {
    Directory(Vec<DirEntry>),
    FileContent(String),
    Binary,
    Loading,
    Empty,
}

pub struct FileManager {
    focus_handle: FocusHandle,
    current_dir: PathBuf,
    entries: Vec<DirEntry>,
    selected_index: usize,
    parent_entries: Vec<DirEntry>,
    preview: Preview,
    show_hidden: bool,
    fs: Arc<dyn fs::Fs>,
}

#[derive(Clone, Debug)]
pub enum FileManagerEvent {
    PathChanged,
}

impl EventEmitter<FileManagerEvent> for FileManager {}

impl FileManager {
    pub fn new(
        initial_dir: PathBuf,
        fs: Arc<dyn fs::Fs>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |this: &mut Self, window, cx| {
            this.reload_entries(window, cx);
        })
        .detach();

        let mut this = Self {
            focus_handle,
            current_dir: initial_dir,
            entries: Vec::new(),
            selected_index: 0,
            parent_entries: Vec::new(),
            preview: Preview::Empty,
            show_hidden: false,
            fs,
        };
        this.reload_entries_sync();
        this
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
        self.update_preview_sync();
    }

    fn reload_entries(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_entries_sync();
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
                    let truncated: String = content.lines().take(50).collect::<Vec<_>>().join("\n");
                    self.preview = Preview::FileContent(truncated);
                }
                Err(_) => {
                    self.preview = Preview::Binary;
                }
            }
        }
    }

    fn navigate_down(&mut self, _: &NavigateDown, window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = cmp::min(self.selected_index + 1, self.entries.len() - 1);
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn navigate_up(&mut self, _: &NavigateUp, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = self.selected_index.saturating_sub(1);
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
            // Open file in editor
            let path = entry.path.clone();
            cx.spawn_in(window, async move |_this, cx| {
                cx.update(|_, cx| {
                    if let Some(workspace_handle) = cx.active_window()
                        .and_then(|w| w.downcast::<workspace::MultiWorkspace>())
                    {
                        workspace_handle.update(cx, |multi_workspace, window, cx| {
                            multi_workspace.workspace().update(cx, |workspace, cx| {
                                workspace.open_abs_path(path, Default::default(), window, cx).detach();
                            });
                        }).ok();
                    }
                });
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

            // Try to select the directory we came from
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
        self.update_preview_sync();
        cx.notify();
    }

    fn go_to_bottom(&mut self, _: &GoToBottom, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn toggle_hidden(&mut self, _: &ToggleHidden, window: &mut Window, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.reload_entries(window, cx);
    }

    fn render_column(
        &self,
        entries: &[DirEntry],
        selected: Option<usize>,
        dimmed: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.colors().surface_background;
        let selected_bg = theme.colors().ghost_element_selected;
        let text_color = if dimmed { Color::Muted } else { Color::Default };
        let dir_color = if dimmed { Color::Muted } else { Color::Accent };

        v_flex()
            .flex_1()
            .overflow_hidden()
            .bg(bg)
            .p(px(4.))
            .children(entries.iter().enumerate().map(|(i, entry)| {
                let is_selected = selected == Some(i);
                let color = if entry.is_dir { dir_color } else { text_color };
                let name = if entry.is_dir {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };

                div()
                    .px(px(6.))
                    .py(px(1.))
                    .when(is_selected, |d| d.bg(selected_bg).rounded_sm())
                    .child(Label::new(name).color(color))
            }))
    }

    fn render_preview(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let bg = theme.colors().surface_background;

        v_flex()
            .flex_1()
            .overflow_hidden()
            .bg(bg)
            .p(px(4.))
            .child(match &self.preview {
                Preview::Directory(entries) => {
                    let names: Vec<_> = entries
                        .iter()
                        .map(|e| {
                            if e.is_dir {
                                format!("{}/", e.name)
                            } else {
                                e.name.clone()
                            }
                        })
                        .collect();
                    div().children(
                        names
                            .into_iter()
                            .map(|n| div().px(px(6.)).py(px(1.)).child(Label::new(n).color(Color::Muted))),
                    )
                }
                Preview::FileContent(content) => div().child(
                    div()
                        .px(px(6.))
                        .py(px(1.))
                        .child(Label::new(content.clone()).color(Color::Muted)),
                ),
                Preview::Binary => {
                    div().child(Label::new("[binary file]").color(Color::Muted))
                }
                Preview::Loading => {
                    div().child(Label::new("Loading...").color(Color::Muted))
                }
                Preview::Empty => {
                    div().child(Label::new("[empty]").color(Color::Muted))
                }
            })
    }
}

impl Focusable for FileManager {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FileManager {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let parent_col = self.render_column(&self.parent_entries, None, true, cx);
        let current_col = self.render_column(&self.entries, Some(self.selected_index), false, cx);
        let preview_col = self.render_preview(cx);

        let theme = cx.theme();
        let border_color = theme.colors().border;
        let dir_display = self.current_dir.display().to_string();

        v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::navigate_down))
            .on_action(cx.listener(Self::navigate_up))
            .on_action(cx.listener(Self::enter_directory))
            .on_action(cx.listener(Self::parent_directory))
            .on_action(cx.listener(Self::go_to_top))
            .on_action(cx.listener(Self::go_to_bottom))
            .on_action(cx.listener(Self::toggle_hidden))
            .child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .child(Label::new(dir_display).color(Color::Muted)),
            )
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_1_4()
                            .h_full()
                            .border_r_1()
                            .border_color(border_color)
                            .child(parent_col),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .border_r_1()
                            .border_color(border_color)
                            .child(current_col),
                    )
                    .child(div().w_1_3().h_full().child(preview_col)),
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
        let fs = self.fs.clone();
        Task::ready(Some(cx.new(|cx| Self::new(dir, fs, window, cx))))
    }

    fn can_split(&self) -> bool {
        true
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
            let is_symlink = e.file_type().ok().map(|ft| ft.is_symlink()).unwrap_or(false);
            Some(DirEntry {
                name,
                path: e.path(),
                is_dir: metadata.is_dir(),
                is_hidden,
                is_symlink,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        // Directories first, then alphabetical
        b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &Open, window, cx| {
            let fs = workspace.app_state().fs.clone();
            let dir = workspace
                .project()
                .read(cx)
                .worktrees(cx)
                .next()
                .map(|wt| wt.read(cx).abs_path().to_path_buf())
                .unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                });

            let file_manager = cx.new(|cx| FileManager::new(dir, fs, window, cx));
            workspace.add_item_to_active_pane(Box::new(file_manager), None, true, window, cx);
        });
    })
    .detach();
}
