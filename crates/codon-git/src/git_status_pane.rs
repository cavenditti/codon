//! Git status as a codon pane (`workspace::Item`).
//!
//! Lists the current repository's working tree under three sections —
//! Staged, Unstaged, Untracked (plus Unmerged when conflicts exist). The
//! pane subscribes to `RepositoryEvent::StatusesChanged` so it stays live
//! while the user edits files in other panes.
//!
//! Keymap (Normal): `j`/`k` move; `Enter` opens the file in an editor
//! pane; `s` stages the entry; `u` unstages it; `g`/`G` jump to top /
//! bottom of the list.

use git::{
    repository::RepoPath,
    status::{FileStatus, StatusCode},
};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyContext, Render,
    SharedString, Subscription, Task, WeakEntity, Window, actions, div, prelude::*,
};
use project::{
    Project,
    git_store::{Repository, RepositoryEvent},
};
use theme::ActiveTheme;
use ui::{Color, Icon, IconName, Label, LabelCommon, LabelSize, h_flex, v_flex};
use workspace::{Item, Workspace, item::ItemEvent};

actions!(
    codon_git,
    [
        /// Open or focus the git status pane in the active workspace.
        OpenStatusPane,
        NavigateUp,
        NavigateDown,
        Open,
        Stage,
        Unstage,
        GoToTop,
        GoToBottom,
    ]
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Staged,
    Unstaged,
    Unmerged,
    Untracked,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Staged => "Staged",
            Section::Unstaged => "Unstaged",
            Section::Unmerged => "Unmerged",
            Section::Untracked => "Untracked",
        }
    }
}

#[derive(Clone)]
struct StatusRow {
    section: Section,
    repo_path: RepoPath,
    /// Single-letter status code (M / A / D / R / C / T / ? / U).
    code: char,
}

pub struct GitStatusPane {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    project: WeakEntity<Project>,
    repository: Option<Entity<Repository>>,
    rows: Vec<StatusRow>,
    selected: usize,
    _repo_sub: Option<Subscription>,
}

#[derive(Clone, Debug)]
pub enum GitStatusPaneEvent {
    RowsChanged,
}

impl EventEmitter<GitStatusPaneEvent> for GitStatusPane {}

impl GitStatusPane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        project: WeakEntity<Project>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The project handle has to be resolved by the caller — this
        // constructor runs inside `cx.new(…)` while the workspace entity
        // is already being updated by the action handler that's opening
        // us, so any `workspace.read(cx)` here double-leases and panics.
        let focus_handle = cx.focus_handle();
        let mut pane = Self {
            focus_handle,
            workspace,
            project,
            repository: None,
            rows: Vec::new(),
            selected: 0,
            _repo_sub: None,
        };
        pane.attach_repository(cx);
        pane
    }

    fn attach_repository(&mut self, cx: &mut Context<Self>) {
        let project = match self.project.upgrade() {
            Some(p) => p,
            None => return,
        };
        let Some(repo) = project.read(cx).active_repository(cx) else {
            self.repository = None;
            self._repo_sub = None;
            self.rows.clear();
            self.selected = 0;
            return;
        };
        self._repo_sub = Some(cx.subscribe(&repo, |this, _repo, event: &RepositoryEvent, cx| {
            if matches!(event, RepositoryEvent::StatusesChanged { .. }) {
                this.rebuild_rows(cx);
            }
        }));
        self.repository = Some(repo);
        self.rebuild_rows(cx);
    }

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let mut rows = Vec::new();
        if let Some(repo) = self.repository.as_ref() {
            for entry in repo.read(cx).status() {
                match entry.status {
                    FileStatus::Ignored => {}
                    FileStatus::Untracked => rows.push(StatusRow {
                        section: Section::Untracked,
                        repo_path: entry.repo_path.clone(),
                        code: '?',
                    }),
                    FileStatus::Unmerged(_) => rows.push(StatusRow {
                        section: Section::Unmerged,
                        repo_path: entry.repo_path.clone(),
                        code: 'U',
                    }),
                    FileStatus::Tracked(t) => {
                        if t.index_status != StatusCode::Unmodified {
                            rows.push(StatusRow {
                                section: Section::Staged,
                                repo_path: entry.repo_path.clone(),
                                code: status_letter(t.index_status),
                            });
                        }
                        if t.worktree_status != StatusCode::Unmodified {
                            rows.push(StatusRow {
                                section: Section::Unstaged,
                                repo_path: entry.repo_path.clone(),
                                code: status_letter(t.worktree_status),
                            });
                        }
                    }
                }
            }
        }
        // Order: Staged, Unstaged, Unmerged, Untracked — same as `git status`.
        rows.sort_by_key(|r| section_order(r.section));
        if self.selected >= rows.len() {
            self.selected = rows.len().saturating_sub(1);
        }
        self.rows = rows;
        cx.emit(GitStatusPaneEvent::RowsChanged);
        cx.notify();
    }

    fn navigate(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
        cx.notify();
    }

    fn go_to(&mut self, position: usize, cx: &mut Context<Self>) {
        if position < self.rows.len() {
            self.selected = position;
            cx.notify();
        }
    }

    fn selected_row(&self) -> Option<&StatusRow> {
        self.rows.get(self.selected)
    }

    fn handle_open(&mut self, _: &Open, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let Some(repo) = self.repository.as_ref() else {
            return;
        };
        let Some(project_path) = repo.read(cx).repo_path_to_project_path(&row.repo_path, cx)
        else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_path(project_path, None, true, window, cx)
                .detach_and_log_err(cx);
        });
    }

    fn handle_stage(&mut self, _: &Stage, _window: &mut Window, cx: &mut Context<Self>) {
        self.stage_or_unstage(true, cx);
    }

    fn handle_unstage(&mut self, _: &Unstage, _window: &mut Window, cx: &mut Context<Self>) {
        self.stage_or_unstage(false, cx);
    }

    fn stage_or_unstage(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        let Some(repo) = self.repository.clone() else {
            return;
        };
        repo.update(cx, |repo, cx| {
            let task: Task<_> = if stage {
                repo.stage_entries(vec![row.repo_path], cx)
            } else {
                repo.unstage_entries(vec![row.repo_path], cx)
            };
            task.detach_and_log_err(cx);
        });
    }

    fn dispatch_context(&self) -> KeyContext {
        let mut ctx = KeyContext::new_with_defaults();
        ctx.add("GitStatus");
        ctx
    }
}

fn status_letter(code: StatusCode) -> char {
    match code {
        StatusCode::Modified => 'M',
        StatusCode::TypeChanged => 'T',
        StatusCode::Added => 'A',
        StatusCode::Deleted => 'D',
        StatusCode::Renamed => 'R',
        StatusCode::Copied => 'C',
        StatusCode::Unmodified => ' ',
    }
}

fn section_order(section: Section) -> u8 {
    match section {
        Section::Staged => 0,
        Section::Unstaged => 1,
        Section::Unmerged => 2,
        Section::Untracked => 3,
    }
}

impl Focusable for GitStatusPane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GitStatusPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let mut last_section: Option<Section> = None;
        let mut visual_idx = 0usize;

        let mut content = v_flex().gap_1().p_2().size_full();

        if self.rows.is_empty() {
            content = content.child(
                Label::new("No changes")
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            );
        }

        for row in &self.rows {
            if last_section != Some(row.section) {
                last_section = Some(row.section);
                content = content.child(
                    Label::new(SharedString::from(row.section.label()))
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                );
            }

            let is_selected = visual_idx == self.selected;
            let path_text = row.repo_path.as_std_path().display().to_string();
            let (icon, icon_color) = section_icon(row.section);

            let row_el = h_flex()
                .gap_2()
                .px_2()
                .py_px()
                .when(is_selected, |el| el.bg(colors.element_selected))
                .child(Icon::new(icon).color(icon_color))
                .child(Label::new(SharedString::from(row.code.to_string())).color(icon_color))
                .child(Label::new(SharedString::from(path_text)));

            content = content.child(row_el);
            visual_idx += 1;
        }

        div()
            .track_focus(&self.focus_handle)
            .key_context(self.dispatch_context())
            .on_action(cx.listener(|this, _: &NavigateDown, _, cx| this.navigate(1, cx)))
            .on_action(cx.listener(|this, _: &NavigateUp, _, cx| this.navigate(-1, cx)))
            .on_action(cx.listener(|this, _: &GoToTop, _, cx| this.go_to(0, cx)))
            .on_action(cx.listener(|this, _: &GoToBottom, _, cx| {
                let last = this.rows.len().saturating_sub(1);
                this.go_to(last, cx);
            }))
            .on_action(cx.listener(Self::handle_open))
            .on_action(cx.listener(Self::handle_stage))
            .on_action(cx.listener(Self::handle_unstage))
            .size_full()
            .child(content)
    }
}

fn section_icon(section: Section) -> (IconName, Color) {
    match section {
        Section::Staged => (IconName::Plus, Color::Success),
        Section::Unstaged => (IconName::Pencil, Color::Warning),
        Section::Unmerged => (IconName::Warning, Color::Error),
        Section::Untracked => (IconName::FileGeneric, Color::Muted),
    }
}

impl Item for GitStatusPane {
    type Event = GitStatusPaneEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        SharedString::from("Git Status")
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::GitBranch))
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            GitStatusPaneEvent::RowsChanged => f(ItemEvent::UpdateBreadcrumbs),
        }
    }
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &OpenStatusPane, window, cx| {
            open_status_pane(workspace, window, cx);
        });
    })
    .detach();
}

fn open_status_pane(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak_workspace = workspace.weak_handle();
    let project = workspace.project().downgrade();
    let pane = cx.new(|cx| GitStatusPane::new(weak_workspace, project, window, cx));
    workspace.add_item_to_active_pane(Box::new(pane), None, true, window, cx);
}
