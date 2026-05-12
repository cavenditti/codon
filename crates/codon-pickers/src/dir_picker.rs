use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use fuzzy::StringMatchCandidate;
use gpui::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, Styled, Subscription, Task, WeakEntity, Window, div,
};
use picker::{Picker, PickerDelegate};
use ui::{
    Color, HighlightedLabel, Icon, IconName, Label, LabelCommon, LabelSize, ListItem,
    ListItemSpacing, StyledExt, Toggleable as _, h_flex, rems, v_flex,
};
use workspace::{ModalView, Workspace};

/// Event emitted when the user confirms the current directory
/// (cmd-Enter on any row, or Enter on the `.` self entry).
#[derive(Clone, Debug)]
pub struct DirSelected {
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
struct PickerDismissed;

#[derive(Clone)]
struct DirCandidate {
    name: String,
    /// Path that pressing Enter resolves to. For `..` and real dirs, this
    /// is the new `current_dir` after descent. For the `.` self entry, it
    /// is the current_dir itself and Enter triggers selection.
    path: PathBuf,
    kind: CandidateKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateKind {
    SelfDir,
    ParentDir,
    Child,
}

pub struct DirPickerDelegate {
    current_dir: PathBuf,
    candidates: Vec<DirCandidate>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
}

impl DirPickerDelegate {
    fn new(start: PathBuf) -> Self {
        let mut this = Self {
            current_dir: PathBuf::new(),
            candidates: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
        };
        this.set_current_dir(start);
        this
    }

    fn set_current_dir(&mut self, dir: PathBuf) {
        self.current_dir = dir;
        self.candidates = build_candidates(&self.current_dir);
        self.matches = self
            .candidates
            .iter()
            .enumerate()
            .map(|(ix, c)| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: Vec::new(),
                string: c.name.clone(),
            })
            .collect();
        self.selected_index = 0;
    }
}

fn build_candidates(dir: &Path) -> Vec<DirCandidate> {
    let mut out = Vec::new();
    out.push(DirCandidate {
        name: ". (use this directory)".into(),
        path: dir.to_path_buf(),
        kind: CandidateKind::SelfDir,
    });
    if let Some(parent) = dir.parent() {
        out.push(DirCandidate {
            name: ".. (parent)".into(),
            path: parent.to_path_buf(),
            kind: CandidateKind::ParentDir,
        });
    }
    if let Ok(read) = std::fs::read_dir(dir) {
        let mut children: Vec<DirCandidate> = read
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let file_type = e.file_type().ok()?;
                if !file_type.is_dir() && !file_type.is_symlink() {
                    return None;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    return None;
                }
                Some(DirCandidate {
                    name,
                    path: e.path(),
                    kind: CandidateKind::Child,
                })
            })
            .collect();
        children.sort_by_key(|c| c.name.to_lowercase());
        out.extend(children);
    }
    out
}

impl EventEmitter<DirSelected> for Picker<DirPickerDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<DirPickerDelegate> {}

impl PickerDelegate for DirPickerDelegate {
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
        Arc::from("Filter directories…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.trim().to_string();
        let candidates: Vec<StringMatchCandidate> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(ix, c)| StringMatchCandidate::new(ix, &c.name))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = std::sync::atomic::AtomicBool::new(false);

        cx.spawn(async move |this, cx| {
            let matches = fuzzy::match_strings(
                &candidates,
                &query,
                false,
                true,
                100,
                &cancel,
                executor,
            )
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

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(candidate) = self.candidates.get(matched.candidate_id) else {
            return;
        };

        // cmd-Enter always confirms the current directory regardless of
        // which row is highlighted.
        if secondary {
            cx.emit(DirSelected {
                path: self.current_dir.clone(),
            });
            return;
        }

        match candidate.kind {
            CandidateKind::SelfDir => {
                cx.emit(DirSelected {
                    path: candidate.path.clone(),
                });
            }
            CandidateKind::ParentDir | CandidateKind::Child => {
                self.set_current_dir(candidate.path.clone());
                self.update_matches(String::new(), window, cx).detach();
                cx.notify();
            }
        }
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(PickerDismissed);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let matched = self.matches.get(ix)?;
        let candidate = self.candidates.get(matched.candidate_id)?;
        let icon = match candidate.kind {
            CandidateKind::SelfDir => IconName::Check,
            CandidateKind::ParentDir => IconName::ArrowUp,
            CandidateKind::Child => IconName::Folder,
        };
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex()
                        .flex_grow()
                        .gap_3()
                        .child(Icon::new(icon).color(Color::Muted))
                        .child(HighlightedLabel::new(
                            matched.string.clone(),
                            matched.positions.clone(),
                        )),
                ),
        )
    }
}

pub struct DirPickerModal {
    picker: Entity<Picker<DirPickerDelegate>>,
    _subscriptions: [Subscription; 2],
}

impl DirPickerModal {
    pub fn new<F>(
        start: PathBuf,
        on_pick: F,
        _workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        F: Fn(PathBuf, &mut Window, &mut App) + 'static,
    {
        let delegate = DirPickerDelegate::new(start);
        let picker =
            cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &DirSelected, window, cx| {
                on_pick(event.path.clone(), window, cx);
                this.dismiss(window, cx);
            },
        );

        let on_dismiss =
            cx.subscribe_in(&picker, window, |this, _, _: &PickerDismissed, window, cx| {
                this.dismiss(window, cx);
            });

        Self {
            picker,
            _subscriptions: [on_select, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn current_dir(&self, cx: &App) -> PathBuf {
        self.picker.read(cx).delegate.current_dir.clone()
    }
}

impl ModalView for DirPickerModal {}
impl EventEmitter<DismissEvent> for DirPickerModal {}

impl Focusable for DirPickerModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for DirPickerModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current_dir(cx);
        div()
            .elevation_3(cx)
            .w(rems(34.))
            .flex_1()
            .overflow_hidden()
            .child(
                v_flex()
                    .child(
                        h_flex().px_3().py_1().child(
                            Label::new(current.display().to_string())
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.picker.clone()),
            )
    }
}
