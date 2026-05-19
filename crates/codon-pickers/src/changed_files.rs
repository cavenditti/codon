//! Changed-files picker — `codon_pickers::ChangedFilesPicker`.
//!
//! A `picker::Picker` delegate over the project's git status, restricted to
//! entries whose `FileStatus` is something the user wants to *jump to* (i.e.
//! has real worktree changes, conflicts, or is untracked). `Unmodified` and
//! `Ignored` are filtered out — they would never appear in `git status` and
//! pinning them in the picker would only crowd out the rows that matter.
//!
//! Confirming a row opens the file in the active pane and scrolls to the
//! first changed hunk (same approach the git panel's `open_file` handler
//! uses).
//!
//! See `TASK:phase-16/pickers-changed-files` and
//! `REQ:codon/helix-pickers#c-changed-files-picker`.

use std::sync::Arc;

use editor::{Direction, Editor};
use fuzzy::StringMatchCandidate;
use gpui::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, WeakEntity,
    Window, actions,
};
use picker::{Picker, PickerDelegate};
use project::{ProjectPath, git_store::StatusEntry};
use ui::{
    Color, HighlightedLabel, Icon, IconName, Label, LabelCommon, LabelSize, ListItem,
    ListItemSpacing, StyledExt, Toggleable as _, h_flex, rems, v_flex,
};
use util::ResultExt as _;
use workspace::{ModalView, Workspace};

use crate::last_picker;
use crate::scaffold::{ModalModeTag, ModalScaffold};

actions!(
    codon_pickers,
    [
        /// Open the changed-files picker — fuzzy-match over `git status`
        /// entries, confirm to jump to the first changed hunk.
        ChangedFilesPicker,
    ]
);

/// Stable identifier of this picker for the [`crate::last_picker`] singleton.
pub(crate) const PICKER_ACTION_NAME: &str = "codon_pickers::ChangedFilesPicker";

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(handle_toggle);
    })
    .detach();
}

fn handle_toggle(
    workspace: &mut Workspace,
    _: &ChangedFilesPicker,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak = workspace.weak_handle();
    let initial = last_picker::take_query_for(cx, PICKER_ACTION_NAME);
    workspace.toggle_modal(window, cx, move |window, cx| {
        ChangedFilesModal::new(weak, initial, window, cx)
    });
}

#[derive(Clone)]
struct ChangedFileRow {
    /// Display label — `git status --short`-style two-letter prefix plus the
    /// repo-relative path. We hold the rendered string verbatim so fuzzy
    /// matching scores against it directly.
    display: String,
    /// Project path the row resolves to. `None` only if the worktree the
    /// entry belonged to has gone away between enumeration and confirm; the
    /// confirm path silently skips those rows.
    project_path: Option<ProjectPath>,
    /// The raw status entry, kept for status-glyph colouring at render time.
    entry: StatusEntry,
}

pub struct ChangedFilesPickerDelegate {
    workspace: WeakEntity<Workspace>,
    rows: Vec<ChangedFileRow>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
    last_query: SharedString,
}

impl ChangedFilesPickerDelegate {
    fn new(workspace: WeakEntity<Workspace>, cx: &mut App) -> Self {
        let rows = collect_rows(&workspace, cx);
        let matches = rows
            .iter()
            .enumerate()
            .map(|(ix, row)| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: Vec::new(),
                string: row.display.clone(),
            })
            .collect();
        Self {
            workspace,
            rows,
            matches,
            selected_index: 0,
            last_query: SharedString::default(),
        }
    }
}

fn collect_rows(workspace: &WeakEntity<Workspace>, cx: &mut App) -> Vec<ChangedFileRow> {
    let Some(workspace) = workspace.upgrade() else {
        return Vec::new();
    };
    let project = workspace.read(cx).project().clone();
    let git_store = project.read(cx).git_store().clone();
    let mut rows: Vec<ChangedFileRow> = Vec::new();
    let repositories: Vec<Entity<project::git_store::Repository>> =
        git_store.read(cx).repositories().values().cloned().collect();
    for repo in repositories {
        let repo_ref = repo.read(cx);
        for entry in repo_ref.cached_status() {
            if !is_picker_visible(&entry.status) {
                continue;
            }
            let project_path = repo_ref.repo_path_to_project_path(&entry.repo_path, cx);
            let glyph = status_glyph(&entry.status);
            let path_string = entry.repo_path.as_unix_str().to_string();
            let display = format!("{glyph}  {path_string}");
            rows.push(ChangedFileRow {
                display,
                project_path,
                entry,
            });
        }
    }
    rows.sort_by(|a, b| a.display.cmp(&b.display));
    rows.dedup_by(|a, b| a.display == b.display);
    rows
}

/// Filter rule from the spec: include any status with real worktree changes
/// (`Modified`, `Added`, `Deleted`, `Renamed`, `Copied`, `TypeChanged`),
/// plus `Untracked` and conflicts. Exclude `Unmodified` (would never appear
/// in `git status` output) and `Ignored` (the user explicitly told git to
/// ignore them — surfacing here would be noise).
fn is_picker_visible(status: &git::status::FileStatus) -> bool {
    use git::status::{FileStatus, StatusCode, TrackedStatus};
    match status {
        FileStatus::Ignored => false,
        FileStatus::Untracked | FileStatus::Unmerged(_) => true,
        FileStatus::Tracked(TrackedStatus {
            index_status,
            worktree_status,
        }) => {
            *index_status != StatusCode::Unmodified || *worktree_status != StatusCode::Unmodified
        }
    }
}

/// Two-letter glyph mirroring `git status --short` output. The index slot
/// is the left character; the worktree slot is the right character. Conflicts
/// collapse to `UU`; untracked is `??`; ignored is `!!` (filtered out by
/// [`is_picker_visible`] before reaching here, but kept for completeness).
fn status_glyph(status: &git::status::FileStatus) -> String {
    use git::status::{FileStatus, TrackedStatus};
    match status {
        FileStatus::Untracked => "??".into(),
        FileStatus::Ignored => "!!".into(),
        FileStatus::Unmerged(_) => "UU".into(),
        FileStatus::Tracked(TrackedStatus {
            index_status,
            worktree_status,
        }) => {
            let i = status_code_glyph(*index_status);
            let w = status_code_glyph(*worktree_status);
            format!("{i}{w}")
        }
    }
}

fn status_code_glyph(code: git::status::StatusCode) -> char {
    use git::status::StatusCode;
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

impl PickerDelegate for ChangedFilesPickerDelegate {
    type ListItem = ListItem;

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
        cx.notify();
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        Arc::from("Filter changed files…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.last_query = SharedString::from(query.clone());
        let trimmed = query.trim().to_string();
        let candidates: Vec<StringMatchCandidate> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| StringMatchCandidate::new(ix, &row.display))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        cx.spawn(async move |this, cx| {
            let matches =
                fuzzy::match_strings(&candidates, &trimmed, false, true, 100, &cancel, executor)
                    .await;
            this.update(cx, |picker, cx| {
                picker.delegate.matches = matches;
                if picker.delegate.selected_index >= picker.delegate.matches.len() {
                    picker.delegate.selected_index = 0;
                }
                cx.notify();
            })
            .ok();
        })
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(row) = self.rows.get(matched.candidate_id) else {
            return;
        };
        let Some(project_path) = row.project_path.clone() else {
            return;
        };
        // The picker is hosted inside a `ChangedFilesModal`. Dropping the
        // last reference to the modal closes it; we emit the open call into
        // the workspace and then leave the dismissal to the surrounding
        // modal subscription.
        let workspace = self.workspace.clone();
        let open_task = workspace.update(cx, |workspace, cx| {
            workspace.open_path(project_path, None, true, window, cx)
        });
        let Ok(open_task) = open_task else {
            cx.emit(PickerDismissed);
            return;
        };
        cx.spawn_in(window, async move |_picker, cx| {
            let item = open_task.await.log_err()?;
            let editor = item.downcast::<Editor>()?;
            let diff_task =
                editor.update(cx, |editor, _cx| editor.wait_for_diff_to_load());
            if let Some(diff_task) = diff_task {
                diff_task.await;
            }
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let snapshot = editor.snapshot(window, cx);
                    editor.go_to_hunk_before_or_after_position(
                        &snapshot,
                        language::Point::new(0, 0),
                        Direction::Next,
                        true,
                        window,
                        cx,
                    );
                });
            })
            .log_err();
            Some(())
        })
        .detach();
        cx.emit(PickerDismissed);
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        last_picker::record_dismissed(cx, PICKER_ACTION_NAME, self.last_query.clone());
        cx.emit(PickerDismissed);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let matched = self.matches.get(ix)?;
        let row = self.rows.get(matched.candidate_id)?;
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex()
                        .flex_grow()
                        .gap_3()
                        .child(Icon::new(status_icon(&row.entry.status)).color(Color::Muted))
                        .child(HighlightedLabel::new(
                            matched.string.clone(),
                            matched.positions.clone(),
                        )),
                ),
        )
    }
}

fn status_icon(status: &git::status::FileStatus) -> IconName {
    use git::status::FileStatus;
    match status {
        FileStatus::Untracked => IconName::SquarePlus,
        FileStatus::Ignored => IconName::EyeOff,
        FileStatus::Unmerged(_) => IconName::Warning,
        FileStatus::Tracked(_) => IconName::FileGit,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PickerDismissed;

impl EventEmitter<PickerDismissed> for Picker<ChangedFilesPickerDelegate> {}

pub struct ChangedFilesModal {
    scaffold: ModalScaffold,
    picker: Entity<Picker<ChangedFilesPickerDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl ChangedFilesModal {
    fn new(
        workspace: WeakEntity<Workspace>,
        initial_query: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = ChangedFilesPickerDelegate::new(workspace, cx);
        let picker = cx.new(|cx| {
            let picker = Picker::uniform_list(delegate, window, cx).modal(false);
            if let Some(query) = initial_query
                && !query.is_empty()
            {
                picker.set_query(query.as_ref(), window, cx);
            }
            picker
        });
        let on_dismiss = cx.subscribe_in(
            &picker,
            window,
            |this, _, _: &PickerDismissed, window, cx| {
                this.dismiss(window, cx);
            },
        );
        Self {
            scaffold,
            picker,
            _subscriptions: vec![on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for ChangedFilesModal {}
impl EventEmitter<DismissEvent> for ChangedFilesModal {}

impl Focusable for ChangedFilesModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for ChangedFilesModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.picker.read(cx).delegate.rows.len();
        gpui::div()
            .elevation_3(cx)
            .w(rems(34.))
            .flex_1()
            .overflow_hidden()
            .child(
                v_flex()
                    .child(
                        h_flex().px_3().py_1().child(
                            Label::new(format!("{count} changed files"))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.picker.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::status::{FileStatus, StatusCode, TrackedStatus, UnmergedStatus, UnmergedStatusCode};

    #[test]
    fn filter_drops_unmodified_and_ignored() {
        assert!(!is_picker_visible(&FileStatus::Ignored));
        let unmodified = FileStatus::Tracked(TrackedStatus {
            index_status: StatusCode::Unmodified,
            worktree_status: StatusCode::Unmodified,
        });
        assert!(!is_picker_visible(&unmodified));
    }

    #[test]
    fn filter_keeps_untracked_modified_added_renamed_deleted_conflict() {
        assert!(is_picker_visible(&FileStatus::Untracked));
        assert!(is_picker_visible(&FileStatus::Unmerged(UnmergedStatus {
            first_head: UnmergedStatusCode::Updated,
            second_head: UnmergedStatusCode::Updated,
        })));
        for code in [
            StatusCode::Modified,
            StatusCode::Added,
            StatusCode::Deleted,
            StatusCode::Renamed,
            StatusCode::Copied,
            StatusCode::TypeChanged,
        ] {
            let status = FileStatus::Tracked(TrackedStatus {
                index_status: code,
                worktree_status: StatusCode::Unmodified,
            });
            assert!(
                is_picker_visible(&status),
                "expected {code:?} to surface in picker"
            );
        }
    }

    #[test]
    fn glyph_matches_git_status_short() {
        let modified = FileStatus::Tracked(TrackedStatus {
            index_status: StatusCode::Unmodified,
            worktree_status: StatusCode::Modified,
        });
        assert_eq!(status_glyph(&modified), " M");
        assert_eq!(status_glyph(&FileStatus::Untracked), "??");
        assert_eq!(
            status_glyph(&FileStatus::Unmerged(UnmergedStatus {
                first_head: UnmergedStatusCode::Updated,
                second_head: UnmergedStatusCode::Updated,
            })),
            "UU"
        );
    }
}
