use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::scaffold::{ModalModeTag, ModalScaffold};
use fuzzy::StringMatchCandidate;
use gpui::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyBinding, ParentElement, Render, Styled, Subscription, Task,
    WeakEntity, Window, actions, div,
};
use picker::{Picker, PickerDelegate};
use ui::{
    Color, FluentBuilder as _, HighlightedLabel, Icon, IconName, Label, LabelCommon, LabelSize,
    ListItem, ListItemSpacing, StyledExt, Toggleable as _, h_flex, rems, v_flex,
};
use workspace::{ModalView, Workspace};

actions!(
    codon_pickers,
    [
        /// Toggle the mark on the focused row in a multi-select DirPicker.
        ToggleMark,
    ]
);

/// Register default keybindings for the dir picker. Idempotent — safe
/// to call multiple times because `cx.bind_keys` appends to a registry.
pub(crate) fn register_default_keybindings(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("space", ToggleMark, Some("DirPickerMulti"))]);
}

/// Event emitted when the user confirms the current directory
/// (cmd-Enter on any row, or Enter on the `.` self entry) in
/// single-select mode.
#[derive(Clone, Debug)]
pub struct DirSelected {
    pub path: PathBuf,
}

/// Event emitted in multi-select mode when the user confirms after
/// marking one or more files/directories.
#[derive(Clone, Debug)]
pub struct FilesSelected {
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct PickerDismissed;

#[derive(Clone)]
struct DirCandidate {
    name: String,
    /// Path that pressing Enter resolves to. For `..` and real dirs, this
    /// is the new `current_dir` after descent. For the `.` self entry, it
    /// is the current_dir itself and Enter triggers selection. For file
    /// children (multi-select only), this is the file's absolute path.
    path: PathBuf,
    kind: CandidateKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateKind {
    SelfDir,
    ParentDir,
    Child,
    /// File children only appear in multi-select mode. They can be
    /// marked/unmarked but pressing Enter on them confirms (returning
    /// the marks or just the focused file if no marks).
    ChildFile,
}

pub struct DirPickerDelegate {
    current_dir: PathBuf,
    candidates: Vec<DirCandidate>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
    multi: bool,
    /// Set of candidate ids (indices into `candidates`) that the user
    /// has marked in multi-select mode. Marks are scoped to the current
    /// directory listing — descending or going up clears them.
    marked: BTreeSet<usize>,
}

impl DirPickerDelegate {
    fn new(start: PathBuf, multi: bool) -> Self {
        let mut this = Self {
            current_dir: PathBuf::new(),
            candidates: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
            multi,
            marked: BTreeSet::new(),
        };
        this.set_current_dir(start);
        this
    }

    fn set_current_dir(&mut self, dir: PathBuf) {
        self.current_dir = dir;
        self.candidates = build_candidates(&self.current_dir, self.multi);
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
        self.marked.clear();
    }

    /// Toggle the mark on the currently focused row. No-op for `.`/`..`
    /// rows or when not in multi-select mode.
    fn toggle_mark_at_selected(&mut self) {
        if !self.multi {
            return;
        }
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let candidate_id = matched.candidate_id;
        let Some(candidate) = self.candidates.get(candidate_id) else {
            return;
        };
        match candidate.kind {
            CandidateKind::Child | CandidateKind::ChildFile => {
                if !self.marked.insert(candidate_id) {
                    self.marked.remove(&candidate_id);
                }
            }
            CandidateKind::SelfDir | CandidateKind::ParentDir => {}
        }
    }

    fn marked_paths(&self) -> Vec<PathBuf> {
        self.marked
            .iter()
            .filter_map(|id| self.candidates.get(*id).map(|c| c.path.clone()))
            .collect()
    }
}

fn build_candidates(dir: &Path, include_files: bool) -> Vec<DirCandidate> {
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
                let is_dir = file_type.is_dir() || file_type.is_symlink();
                let is_file = file_type.is_file();
                if !is_dir && !(include_files && is_file) {
                    return None;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    return None;
                }
                let kind = if is_dir {
                    CandidateKind::Child
                } else {
                    CandidateKind::ChildFile
                };
                Some(DirCandidate {
                    name,
                    path: e.path(),
                    kind,
                })
            })
            .collect();
        children.sort_by(|a, b| {
            // Directories first, then files; within each group, case-
            // insensitive alphabetical.
            let a_is_dir = matches!(a.kind, CandidateKind::Child);
            let b_is_dir = matches!(b.kind, CandidateKind::Child);
            b_is_dir
                .cmp(&a_is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        out.extend(children);
    }
    out
}

impl EventEmitter<DirSelected> for Picker<DirPickerDelegate> {}
impl EventEmitter<FilesSelected> for Picker<DirPickerDelegate> {}
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
        if self.multi {
            Arc::from("Filter entries (space to mark, enter to confirm)…")
        } else {
            Arc::from("Filter directories…")
        }
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
            let matches =
                fuzzy::match_strings(&candidates, &query, false, true, 100, &cancel, executor)
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

        if self.multi {
            // In multi-select mode: cmd-Enter confirms the marks (or
            // current_dir if nothing is marked); Enter on a directory
            // descends; Enter on a file or on `.` confirms the marks
            // (falling back to the focused entry if no marks).
            if secondary {
                let paths = if self.marked.is_empty() {
                    vec![self.current_dir.clone()]
                } else {
                    self.marked_paths()
                };
                cx.emit(FilesSelected { paths });
                return;
            }
            match candidate.kind {
                CandidateKind::Child | CandidateKind::ParentDir => {
                    self.set_current_dir(candidate.path.clone());
                    self.update_matches(String::new(), window, cx).detach();
                    cx.notify();
                }
                CandidateKind::SelfDir | CandidateKind::ChildFile => {
                    let paths = if self.marked.is_empty() {
                        vec![candidate.path.clone()]
                    } else {
                        self.marked_paths()
                    };
                    cx.emit(FilesSelected { paths });
                }
            }
            return;
        }

        // Single-select mode. cmd-Enter always confirms the current
        // directory regardless of which row is highlighted.
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
            CandidateKind::ChildFile => {
                // Files don't appear in single-select listings, but
                // guard anyway.
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
            CandidateKind::ChildFile => IconName::File,
        };
        let is_marked = self.multi && self.marked.contains(&matched.candidate_id);
        let mark_icon = if self.multi {
            let icon_name = if is_marked {
                IconName::Check
            } else {
                IconName::Circle
            };
            let color = match candidate.kind {
                CandidateKind::Child | CandidateKind::ChildFile => {
                    if is_marked {
                        Color::Accent
                    } else {
                        Color::Muted
                    }
                }
                CandidateKind::SelfDir | CandidateKind::ParentDir => Color::Disabled,
            };
            Some(Icon::new(icon_name).color(color))
        } else {
            None
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
                        .when_some(mark_icon, |this, icon| this.child(icon))
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
    scaffold: ModalScaffold,
    picker: Entity<Picker<DirPickerDelegate>>,
    multi: bool,
    _subscriptions: Vec<Subscription>,
}

impl DirPickerModal {
    /// Single-select directory picker. Enter on `.` or cmd-Enter on any
    /// row emits `DirSelected` (a single `PathBuf`).
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
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = DirPickerDelegate::new(start, false);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &DirSelected, window, cx| {
                on_pick(event.path.clone(), window, cx);
                this.dismiss(window, cx);
            },
        );

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
            multi: false,
            _subscriptions: vec![on_select, on_dismiss],
        }
    }

    /// Multi-select picker that also shows files. Space toggles the
    /// mark on the focused row; Enter on a directory descends; Enter on
    /// a file (or cmd-Enter anywhere) emits `FilesSelected` with the
    /// marked paths (falling back to the focused entry when no marks
    /// are set).
    pub fn new_multi<F>(
        start: PathBuf,
        on_pick: F,
        _workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        F: Fn(Vec<PathBuf>, &mut Window, &mut App) + 'static,
    {
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = DirPickerDelegate::new(start, true);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &FilesSelected, window, cx| {
                on_pick(event.paths.clone(), window, cx);
                this.dismiss(window, cx);
            },
        );

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
            multi: true,
            _subscriptions: vec![on_select, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn current_dir(&self, cx: &App) -> PathBuf {
        self.picker.read(cx).delegate.current_dir.clone()
    }

    fn handle_toggle_mark(&mut self, _: &ToggleMark, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.multi {
            return;
        }
        self.picker.update(cx, |picker, cx| {
            picker.delegate.toggle_mark_at_selected();
            cx.notify();
        });
    }
}

impl ModalView for DirPickerModal {}
impl EventEmitter<DismissEvent> for DirPickerModal {}

impl Focusable for DirPickerModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a delegate at a path the listing routine can't read. The
    /// `.` and `..` header rows are deterministic — we exercise just
    /// those without touching the filesystem.
    fn delegate_at_unreadable(multi: bool) -> DirPickerDelegate {
        // Use a synthetic absolute path that does not exist on disk. The
        // `build_candidates` `read_dir` branch returns Err and is skipped,
        // leaving just the `.`/`..` rows.
        DirPickerDelegate::new(
            PathBuf::from("/codon-tests/synthetic/does/not/exist"),
            multi,
        )
    }

    #[test]
    fn build_candidates_emits_self_and_parent_for_nested_dir() {
        // For a path with a parent we expect `.` then `..` as the first
        // two header rows. Real children may or may not be present (the
        // path is fabricated and read_dir fails silently).
        let candidates =
            build_candidates(Path::new("/codon-tests/synthetic/does/not/exist"), false);
        assert!(candidates.len() >= 2);
        assert_eq!(candidates[0].kind, CandidateKind::SelfDir);
        assert_eq!(candidates[1].kind, CandidateKind::ParentDir);
    }

    #[test]
    fn build_candidates_skips_parent_at_filesystem_root() {
        // The root has no parent — the `..` row must be omitted so the
        // user cannot accidentally navigate above it. Real child rows
        // may or may not be present depending on the test host, so
        // assert the *absence* of `ParentDir` rather than a list length.
        let candidates = build_candidates(Path::new("/"), false);
        assert_eq!(candidates[0].kind, CandidateKind::SelfDir);
        assert!(
            !candidates
                .iter()
                .any(|c| c.kind == CandidateKind::ParentDir),
            "filesystem root must not emit a ParentDir row"
        );
    }

    #[test]
    fn toggle_mark_at_selected_is_noop_in_single_select_mode() {
        let mut d = delegate_at_unreadable(false);
        d.toggle_mark_at_selected();
        assert!(
            d.marked.is_empty(),
            "single-select must never accumulate marks"
        );
    }

    #[test]
    fn toggle_mark_at_selected_skips_self_and_parent_rows() {
        // `.` is row 0; `..` is row 1. Both must stay unmarkable even in
        // multi-select mode — they are pseudo-rows, not selectable
        // entries.
        let mut d = delegate_at_unreadable(true);
        // Row 0 = SelfDir.
        d.selected_index = 0;
        d.toggle_mark_at_selected();
        assert!(d.marked.is_empty());
        // Row 1 = ParentDir.
        d.selected_index = 1;
        d.toggle_mark_at_selected();
        assert!(d.marked.is_empty());
    }

    #[test]
    fn toggle_mark_at_selected_handles_out_of_range_index_gracefully() {
        let mut d = delegate_at_unreadable(true);
        d.selected_index = 999;
        d.toggle_mark_at_selected();
        assert!(d.marked.is_empty());
    }

    #[test]
    fn marked_paths_empty_when_no_marks() {
        let d = delegate_at_unreadable(true);
        assert!(d.marked_paths().is_empty());
    }

    #[test]
    fn marked_paths_filters_unknown_ids() {
        let mut d = delegate_at_unreadable(true);
        // Inject a stale id pointing past the current candidate list.
        // `marked_paths` filter_maps via `get`, so the stale id is dropped.
        d.marked.insert(9999);
        assert!(d.marked_paths().is_empty());
    }

    #[test]
    fn candidate_kind_self_dir_is_distinct_from_parent_dir() {
        // Trivial but pinned: the four-variant enum must keep all four
        // variants distinct so the `match` arms in `confirm` don't
        // silently collapse.
        assert_ne!(CandidateKind::SelfDir, CandidateKind::ParentDir);
        assert_ne!(CandidateKind::Child, CandidateKind::ChildFile);
    }
}

impl Render for DirPickerModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current_dir(cx);
        let mut root = div()
            .elevation_3(cx)
            .w(rems(34.))
            .flex_1()
            .overflow_hidden();
        if self.multi {
            root = root
                .key_context("DirPickerMulti")
                .on_action(cx.listener(Self::handle_toggle_mark));
        }
        root.child(
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
