use codon_mode::{CodonModeTracker, ObjectKind, PaneMode, Selection, SelectionSource};
use fs::{copy_recursive, CopyOptions, RenameOptions};
use git::status::FileStatus;
use gpui::{
    actions, prelude::*, Action, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle,
    Focusable, KeyContext, ScrollStrategy, SharedString, Task, UniformListScrollHandle, WeakEntity,
    Window,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::cmp;
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ui::{Icon, IconName};
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
        Paste,
        PasteOverwrite,
        BulkRename,
        HistoryBack,
        HistoryForward,
    ]
);

/// Codon-wide "navigate the file manager here" action. The payload is an
/// absolute path: the handler opens (or focuses) the most-recently-active
/// FM pane, navigates it to `path.parent()`, then selects the matching
/// entry. Used by phase-7 search pickers and phase-8 symlink-follow.
#[derive(Clone, Debug, PartialEq, Default, Deserialize, JsonSchema, Action)]
#[action(namespace = codon_fm)]
#[serde(deny_unknown_fields)]
pub struct Reveal(pub PathBuf);

/// Open the FM's `:cd <path>` input bar. Optional `seed` pre-fills the
/// query — used by the `:cd` palette command. Tab in the input bar
/// extends with longest-common-prefix; Enter resolves and navigates.
#[derive(Clone, Debug, PartialEq, Default, Deserialize, JsonSchema, Action)]
#[action(namespace = codon_fm)]
#[serde(deny_unknown_fields)]
pub struct GotoPath(#[serde(default)] pub String);

const HISTORY_CAP: usize = 64;

/// In-memory clipboard for codon's file manager. Distinct from the OS
/// clipboard so the user can `y` paths here and still paste text into a
/// terminal pane from the system clipboard.
#[derive(Clone)]
pub(crate) enum FmClipboard {
    Empty,
    Yank(Vec<PathBuf>),
    Cut(Vec<PathBuf>),
}

impl FmClipboard {
    fn is_empty(&self) -> bool {
        matches!(self, FmClipboard::Empty)
    }

    fn paths(&self) -> &[PathBuf] {
        match self {
            FmClipboard::Empty => &[],
            FmClipboard::Yank(paths) | FmClipboard::Cut(paths) => paths,
        }
    }
}

#[derive(Clone)]
pub(crate) struct DirEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) is_hidden: bool,
    pub(crate) is_symlink: bool,
    pub(crate) size: u64,
    pub(crate) git_status: Option<FileStatus>,
    pub(crate) mtime: Option<std::time::SystemTime>,
    pub(crate) btime: Option<std::time::SystemTime>,
    pub(crate) mode: Option<u32>,
    pub(crate) uid: Option<u32>,
    pub(crate) gid: Option<u32>,
}

#[derive(Clone)]
pub(crate) enum Preview {
    Directory(Vec<DirEntry>),
    FileContent(String),
    Archive(ArchiveListing),
    Image(ImageInfo),
    Binary(BinaryInfo),
    Empty,
}

/// Snapshot used to render an image in the preview pane. `dimensions`
/// is read cheaply from the file header so the placeholder fallback can
/// still surface useful metadata even when the decoder refuses the
/// payload at render time.
#[derive(Clone)]
pub(crate) struct ImageInfo {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) mime: String,
    pub(crate) dimensions: Option<(u32, u32)>,
}

/// Header + first-bytes snapshot used to render a non-text, non-image,
/// non-archive file in the preview column. `head` carries at most 256
/// bytes so the hex dump is bounded.
#[derive(Clone)]
pub(crate) struct BinaryInfo {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) mime: String,
    pub(crate) head: Vec<u8>,
}

/// One entry inside an archive. `size` is the *uncompressed* size when
/// the format reports one (zip, tar); other formats leave it `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArchiveEntry {
    pub(crate) name: String,
    pub(crate) size: Option<u64>,
}

/// Truncated listing of an archive's entries.  When the archive has more
/// entries than `ARCHIVE_ENTRIES_CAP`, `extra` carries the leftover
/// count so the view can render a `… N more` line.
#[derive(Clone)]
pub(crate) struct ArchiveListing {
    pub(crate) entries: Vec<ArchiveEntry>,
    pub(crate) extra: usize,
}

pub struct FileManager {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) workspace: WeakEntity<Workspace>,
    pub(crate) mode: PaneMode,
    pub(crate) current_dir: PathBuf,
    pub(crate) entries: Vec<DirEntry>,
    pub(crate) selected_index: usize,
    pub(crate) marked: BTreeSet<usize>,
    pub(crate) parent_entries: Vec<DirEntry>,
    pub(crate) preview: Preview,
    pub(crate) show_hidden: bool,
    pub(crate) fs: Arc<dyn fs::Fs>,
    pub(crate) scroll_handle: UniformListScrollHandle,
    pub(crate) visible_lines: usize,
    pub(crate) pending_input: Option<PendingInput>,
    pub(crate) filter_query: String,
    pub(crate) entries_unfiltered: Option<Vec<DirEntry>>,
    pub(crate) error_message: Option<String>,
    pub(crate) error_gen: u64,
    pub(crate) clipboard: FmClipboard,
    pub(crate) back_stack: VecDeque<PathBuf>,
    pub(crate) forward_stack: VecDeque<PathBuf>,
    pub(crate) pending_chord: Option<char>,
    pub(crate) visual_anchor: Option<usize>,
    pub(crate) sort: crate::prefs::SortMode,
    pub(crate) reverse: bool,
    pub(crate) line_mode: crate::prefs::LineMode,
    pub(crate) show_gitignored: bool,
    pub(crate) preview_fraction: f32,
    /// Last committed find query (from `/` or `?`). `n` and `N` walk
    /// forward / backward through matches of this pattern. None until
    /// the user commits a find via Enter.
    pub(crate) last_find_pattern: Option<String>,
}

#[derive(Clone)]
pub(crate) enum PendingInput {
    CreateFile(String),
    CreateDirectory(String),
    Rename { original: PathBuf, new_name: String },
    Filter,
    /// `P` overwrite prompt: show how many paths would clobber existing
    /// entries, then wait for y/n. The full plan is kept so the single
    /// confirmation applies to the whole batch.
    ConfirmOverwrite { plan: Vec<PasteEntry>, is_cut: bool },
    /// `D` with marks: confirm before trashing the whole marked set.
    /// `targets` carries the snapshot of paths so the prompt is stable
    /// even if the listing changes mid-input.
    ConfirmDeleteMarked { targets: Vec<PathBuf> },
    /// `R` with marks: input-bar pattern using `{}` as a counter
    /// placeholder. `targets` is the snapshot of marked paths, in
    /// display order, captured at the moment `R` was pressed.
    BulkRename { pattern: String, targets: Vec<PathBuf> },
    /// `:cd <path>` prompt — Tab extends with longest-common-prefix of
    /// filesystem matches; Enter resolves and navigates.
    GotoPath { query: String },
    Chmod {
        input: String,
        targets: Vec<(PathBuf, Option<u32>)>,
    },
    FindForward { query: String, origin_index: usize },
    FindBackward { query: String, origin_index: usize },
}

/// One unit of work for a paste operation: where the bytes come from and
/// where they should land. `destination_exists` lets the paste handler
/// distinguish "fresh write" from "user already confirmed overwrite".
#[derive(Clone)]
pub(crate) struct PasteEntry {
    pub(crate) source: PathBuf,
    pub(crate) destination: PathBuf,
    pub(crate) destination_exists: bool,
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

        let prefs = cx.try_global::<crate::prefs::FmPrefs>().cloned().unwrap_or_default();

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
            clipboard: FmClipboard::Empty,
            back_stack: VecDeque::new(),
            forward_stack: VecDeque::new(),
            pending_chord: None,
            visual_anchor: None,
            sort: prefs.sort,
            reverse: prefs.reverse,
            line_mode: prefs.line_mode,
            show_gitignored: prefs.show_gitignored,
            preview_fraction: crate::prefs::clamp_fraction(prefs.preview_fraction),
            last_find_pattern: None,
        };
        this.reload_entries_sync();
        this
    }

    pub(crate) fn read_dir_options(&self) -> ReadDirOptions {
        ReadDirOptions {
            show_hidden: self.show_hidden,
            sort: self.sort,
            reverse: self.reverse,
        }
    }

    pub(crate) fn push_history_back(&mut self, dir: PathBuf) {
        if self.back_stack.back() == Some(&dir) {
            return;
        }
        self.back_stack.push_back(dir);
        while self.back_stack.len() > HISTORY_CAP {
            self.back_stack.pop_front();
        }
    }

    pub(crate) fn push_history_forward(&mut self, dir: PathBuf) {
        self.forward_stack.push_back(dir);
        while self.forward_stack.len() > HISTORY_CAP {
            self.forward_stack.pop_front();
        }
    }

    pub(crate) fn surface_error(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
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

    pub(crate) fn dispatch_context(&self) -> KeyContext {
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
        let opts = self.read_dir_options();
        self.entries = read_dir_sync(&self.current_dir, opts);
        self.parent_entries = self
            .current_dir
            .parent()
            .map(|p| read_dir_sync(p, opts))
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

    pub(crate) fn select_entry_by_name(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(idx) = self.entries.iter().position(|e| e.name == name) {
            self.selected_index = idx;
            self.ensure_visible();
            self.update_preview_sync();
            cx.notify();
        }
    }

    /// Navigate to `target_dir` and optionally select an entry by name.
    /// Called by the `codon_fm::Reveal` action so any pane can ask the
    /// file manager to surface a path.
    pub(crate) fn reveal_path(
        &mut self,
        target_dir: PathBuf,
        select_name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target_dir != self.current_dir {
            self.push_history_back(self.current_dir.clone());
            self.forward_stack.clear();
            self.current_dir = target_dir;
            self.selected_index = 0;
            self.reload_entries(window, cx);
        }
        if let Some(name) = select_name {
            self.select_entry_by_name(&name, cx);
        }
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
        self.apply_gitignore_filter();
    }

    /// `zg` toggle: when `show_gitignored` is false, drop entries whose
    /// `git_status == Ignored` from both the current and parent columns.
    /// Orthogonal to the `.` (hidden-files) toggle — a `.gitignore`'d
    /// hidden file shows only when both toggles allow it. Selection is
    /// re-anchored to the same name if it survives the filter.
    fn apply_gitignore_filter(&mut self) {
        if self.show_gitignored {
            return;
        }
        let was_filtering = self.entries_unfiltered.is_some();
        let selected_name = self
            .entries
            .get(self.selected_index)
            .map(|e| e.name.clone());
        self.entries
            .retain(|e| !matches!(e.git_status, Some(FileStatus::Ignored)));
        self.parent_entries
            .retain(|e| !matches!(e.git_status, Some(FileStatus::Ignored)));
        if !was_filtering {
            if let Some(name) = selected_name
                && let Some(idx) = self.entries.iter().position(|e| e.name == name)
            {
                self.selected_index = idx;
            } else {
                self.selected_index = cmp::min(
                    self.selected_index,
                    self.entries.len().saturating_sub(1),
                );
            }
        }
    }

    pub(crate) fn update_preview_sync(&mut self) {
        let Some(entry) = self.entries.get(self.selected_index) else {
            self.preview = Preview::Empty;
            return;
        };

        if entry.is_dir {
            let children = read_dir_sync(&entry.path, self.read_dir_options());
            self.preview = Preview::Directory(children);
            return;
        }

        let path = entry.path.clone();
        let name = entry.name.clone();
        let size = entry.size;

        if is_image_path(&path) {
            self.preview = Preview::Image(read_image_info(&path, name, size));
            return;
        }

        if is_archive_path(&path) {
            if let Some(listing) = read_archive_listing(&path) {
                self.preview = Preview::Archive(listing);
                return;
            }
            // Recognised extension but the archive failed to open — fall
            // through to the binary fallback so the user still sees the
            // header / hex dump instead of a silent blank pane.
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let truncated: String = content.lines().take(80).collect::<Vec<_>>().join("\n");
                self.preview = Preview::FileContent(truncated);
            }
            Err(_) => {
                self.preview = Preview::Binary(read_binary_info(&path, name, size));
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
            self.refresh_visual_marks();
            self.update_preview_sync();
            cx.notify();
        }
    }

    fn navigate_up(&mut self, _: &NavigateUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = self.selected_index.saturating_sub(1);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.refresh_visual_marks();
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
            self.push_history_back(self.current_dir.clone());
            self.forward_stack.clear();
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
        if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
            let old_dir_name = self
                .current_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string());
            self.push_history_back(self.current_dir.clone());
            self.forward_stack.clear();
            self.current_dir = parent;
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
        // A bare `v` returns the FM to single-mark muscle memory; the
        // existing visual-range marks are preserved so the toggle still
        // acts on the current row as expected.
        self.visual_anchor = None;
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

    /// `uv` chord: drop every mark. Distinct from a single `v` toggle so
    /// the user can wipe a large mark set in two keystrokes without
    /// having to scroll back over every previously-marked row.
    pub(crate) fn clear_marks(&mut self, cx: &mut Context<Self>) {
        if self.marked.is_empty() && self.visual_anchor.is_none() {
            return;
        }
        self.visual_anchor = None;
        self.marked.clear();
        cx.notify();
    }

    /// `ctrl-a`: mark every entry in the current visible listing.
    /// `self.entries` is already post-filter / post-show_hidden, so the
    /// "visible window" is exactly that vector.
    pub(crate) fn select_all_visible(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        self.visual_anchor = None;
        self.marked = (0..self.entries.len()).collect();
        cx.notify();
    }

    /// `ctrl-r`: flip each visible index's membership in `marked` (the
    /// set-symmetric-difference against the visible window). Entries
    /// outside the visible window — which today is empty by construction,
    /// since marks can only be created from rendered rows — keep their
    /// existing state, so the operation remains correct if that invariant
    /// is ever relaxed.
    pub(crate) fn invert_marks_visible(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        self.visual_anchor = None;
        let visible = 0..self.entries.len();
        let mut next: BTreeSet<usize> = self
            .marked
            .iter()
            .copied()
            .filter(|i| !visible.contains(i))
            .collect();
        for i in visible {
            if !self.marked.contains(&i) {
                next.insert(i);
            }
        }
        self.marked = next;
        cx.notify();
    }

    /// `V` (shift-v): enter visual-line selection mode anchored at the
    /// current cursor. The anchor row is marked immediately; subsequent
    /// j / k navigation extends or shrinks the marked range from there.
    /// Any prior mark set is replaced so the user sees only the sweep
    /// they are actively performing.
    pub(crate) fn start_visual_range(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        let anchor = self.selected_index;
        self.visual_anchor = Some(anchor);
        self.marked.clear();
        self.marked.insert(anchor);
        cx.notify();
    }

    /// `Esc` / `Enter` in visual mode: exit visual-line selection but
    /// keep the marks that were swept. The committed set is what
    /// subsequent verbs (y / d / D / R / p) operate on.
    pub(crate) fn commit_visual_range(&mut self, cx: &mut Context<Self>) {
        self.visual_anchor = None;
        cx.notify();
    }

    /// Refresh the marked span to the inclusive range
    /// `min(anchor, cursor)..=max(anchor, cursor)`. Called from the j/k
    /// navigation paths after `selected_index` has been updated, when —
    /// and only when — `visual_anchor` is set.
    pub(crate) fn refresh_visual_marks(&mut self) {
        let Some(anchor) = self.visual_anchor else {
            return;
        };
        let cursor = self.selected_index;
        let lo = cmp::min(anchor, cursor);
        let hi = cmp::max(anchor, cursor);
        self.marked.clear();
        for i in lo..=hi {
            self.marked.insert(i);
        }
    }

    /// `y` in Normal mode: store the current entry — or the whole marked
    /// set if any — in the file-manager-local "yank" clipboard. `p` then
    /// copies; `P` copies with overwrite confirmation. The OS clipboard is
    /// not touched so terminal/agent panes can paste real text instead of
    /// a path string.
    fn yank_to_clipboard(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.current_targets();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        self.clipboard = FmClipboard::Yank(paths);
        self.marked.clear();
        self.surface_error(format!("Yanked {count} entr{}", plural_y(count)), cx);
        cx.notify();
    }

    /// `d` in Normal mode: mark the current entry (or marked set) for cut.
    /// Actual filesystem rename happens on `p`/`P`. Deletion is bound to
    /// `D` (shift-d) instead — see `delete_entry`.
    fn cut_to_clipboard(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.current_targets();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        self.clipboard = FmClipboard::Cut(paths);
        self.marked.clear();
        self.surface_error(format!("Cut {count} entr{}", plural_y(count)), cx);
        cx.notify();
    }

    /// `Y` (shift-y): write the current path(s) to the OS clipboard as a
    /// newline-joined string. Useful for pasting into a terminal pane.
    fn copy_path_to_os_clipboard(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.current_targets();
        if paths.is_empty() {
            return;
        }
        let joined = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(joined));
    }

    /// Snapshot of which paths the next clipboard / file operation should
    /// apply to: the marked set if non-empty, otherwise just the focused
    /// entry. Returns owned PathBufs so callers can move the list into an
    /// async task without borrowing `self`.
    fn current_targets(&self) -> Vec<PathBuf> {
        if self.marked.is_empty() {
            self.entries
                .get(self.selected_index)
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            self.marked
                .iter()
                .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
                .collect()
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

    pub(crate) fn handle_cancel(
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

    pub(crate) fn start_goto_path(
        &mut self,
        seed: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::GotoPath { query: seed });
        cx.notify();
    }

    /// `/` enters find-forward. On each keystroke (handled in
    /// `handle_insert_key`), the cursor jumps to the next entry whose
    /// name contains the query substring (case-insensitive). Enter
    /// commits the query as `last_find_pattern` so `n` / `N` can repeat.
    fn start_find_forward(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::FindForward {
            query: String::new(),
            origin_index: self.selected_index,
        });
        cx.notify();
    }

    /// `?` enters find-backward. Same UX as forward but the per-keystroke
    /// jump walks backward through `entries`.
    fn start_find_backward(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::FindBackward {
            query: String::new(),
            origin_index: self.selected_index,
        });
        cx.notify();
    }

    /// Search forward from `start` (exclusive) and wrap to the beginning
    /// if no match found before end-of-list. Returns `None` only when no
    /// entry contains `needle`.
    fn find_forward_from(&self, start: usize, needle: &str) -> Option<usize> {
        if self.entries.is_empty() || needle.is_empty() {
            return None;
        }
        let needle = needle.to_lowercase();
        let len = self.entries.len();
        for offset in 1..=len {
            let idx = (start + offset) % len;
            if self.entries[idx].name.to_lowercase().contains(&needle) {
                return Some(idx);
            }
        }
        None
    }

    /// Search backward from `start` (exclusive) and wrap to the end if
    /// no match found before index 0.
    fn find_backward_from(&self, start: usize, needle: &str) -> Option<usize> {
        if self.entries.is_empty() || needle.is_empty() {
            return None;
        }
        let needle = needle.to_lowercase();
        let len = self.entries.len();
        for offset in 1..=len {
            let idx = (start + len - (offset % len)) % len;
            if self.entries[idx].name.to_lowercase().contains(&needle) {
                return Some(idx);
            }
        }
        None
    }

    /// Per-keystroke incremental match for the find prompt. Starts from
    /// `origin_index - 1` (forward) or `+1` (backward) so the first
    /// character anchors against the cursor's pre-find position and lands
    /// on the nearest match in the chosen direction.
    fn apply_find_preview(&mut self, query: &str, origin_index: usize, forward: bool) {
        if query.is_empty() {
            self.selected_index = cmp::min(origin_index, self.entries.len().saturating_sub(1));
            self.ensure_visible();
            self.update_preview_sync();
            return;
        }
        let len = self.entries.len();
        if len == 0 {
            return;
        }
        let anchor = if forward {
            (origin_index + len - 1) % len
        } else {
            (origin_index + 1) % len
        };
        let found = if forward {
            self.find_forward_from(anchor, query)
        } else {
            self.find_backward_from(anchor, query)
        };
        if let Some(idx) = found {
            self.selected_index = idx;
            self.ensure_visible();
            self.update_preview_sync();
        }
    }

    /// `n` (Normal): walk forward through matches of the last committed
    /// find pattern. No-op if no pattern is committed.
    fn find_next(&mut self, cx: &mut Context<Self>) {
        let Some(needle) = self.last_find_pattern.clone() else {
            return;
        };
        if let Some(idx) = self.find_forward_from(self.selected_index, &needle) {
            self.selected_index = idx;
            self.ensure_visible();
            self.update_preview_sync();
            cx.notify();
        }
    }

    /// `N` (Normal): walk backward through matches of the last committed
    /// find pattern. No-op if no pattern is committed.
    fn find_prev(&mut self, cx: &mut Context<Self>) {
        let Some(needle) = self.last_find_pattern.clone() else {
            return;
        };
        if let Some(idx) = self.find_backward_from(self.selected_index, &needle) {
            self.selected_index = idx;
            self.ensure_visible();
            self.update_preview_sync();
            cx.notify();
        }
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
        let targets = self.current_targets();

        if targets.is_empty() {
            return;
        }

        // With marks, the batch is destructive enough to warrant a
        // confirmation prompt routed through the same input-bar pattern
        // as `P`'s overwrite confirm. Single-entry delete keeps its
        // immediate behavior to preserve the fm-copy-paste UX.
        if !self.marked.is_empty() && targets.len() > 1 {
            self.mode = PaneMode::Insert;
            self.pending_input = Some(PendingInput::ConfirmDeleteMarked { targets });
            cx.notify();
            return;
        }

        self.execute_delete(targets, window, cx);
    }

    fn execute_delete(
        &mut self,
        targets: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// `R` (shift-r): bulk-rename the marked set using a pattern that
    /// includes `{}` as a counter placeholder, e.g. `screenshot-{}.png`.
    /// With no marks this is a no-op (single `r` already covers single
    /// rename). The initial pattern seeds from the first marked entry's
    /// extension so the user keeps the original file type by default.
    fn start_bulk_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.marked.is_empty() {
            return;
        }
        let targets: Vec<PathBuf> = self
            .marked
            .iter()
            .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
            .collect();
        if targets.is_empty() {
            return;
        }
        let pattern = default_bulk_rename_pattern(&targets[0]);
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::BulkRename { pattern, targets });
        cx.notify();
    }

    /// `cw` chord: capture the marked entries' paths (or the focused
    /// entry, if no marks) and open the bulk-rename editor in the
    /// active workspace pane. The roundtrip — opening the buffer,
    /// observing its close, diffing, applying renames — lives in
    /// `bulk_rename_editor.rs`.
    fn start_bulk_rename_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let weak_self = cx.weak_entity();
        crate::bulk_rename_editor::open_bulk_rename_editor(
            self.workspace.clone(),
            self.fs.clone(),
            targets,
            weak_self,
            window,
            cx,
        );
    }

    /// Called from the bulk-rename editor's release hook. We're back
    /// on the main thread but without a `Window` — the FM's render
    /// loop only needs an updated entry list and a `cx.notify()` to
    /// repaint, so we drop the git-status refresh until the next
    /// focus event.
    pub(crate) fn reload_entries_after_bulk_rename(&mut self, cx: &mut Context<Self>) {
        self.reload_entries_sync();
        cx.emit(FileManagerEvent::PathChanged);
        cx.notify();
    }

    /// `cm` chord: snapshot the affected paths + their current mode and
    /// open the chmod input bar. If nothing is marked, fall back to the
    /// focused entry — `cm` should never be a silent no-op when the user
    /// pressed `m` after `c`.
    fn start_bulk_chmod(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let targets: Vec<(PathBuf, Option<u32>)> = if self.marked.is_empty() {
            self.entries
                .get(self.selected_index)
                .map(|e| vec![(e.path.clone(), e.mode)])
                .unwrap_or_default()
        } else {
            self.marked
                .iter()
                .filter_map(|&i| {
                    self.entries
                        .get(i)
                        .map(|e| (e.path.clone(), e.mode))
                })
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::Chmod {
            input: String::new(),
            targets,
        });
        cx.notify();
    }

    /// Apply `input` (octal or symbolic) to every snapshot path. On
    /// Windows this is a no-op + toast — `fs::Fs::set_permissions` itself
    /// is a no-op on non-unix, but we surface a message so the user
    /// knows the keystroke landed and was ignored.
    fn execute_bulk_chmod(
        &mut self,
        input: String,
        targets: Vec<(PathBuf, Option<u32>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(not(unix))]
        {
            let _ = (input, targets, window);
            self.surface_error("chmod is a unix-only operation", cx);
            return;
        }

        #[cfg(unix)]
        {
            let trimmed = input.trim();
            let plan: Result<Vec<(PathBuf, u32)>, String> = targets
                .iter()
                .map(|(path, current)| {
                    apply_chmod_input(trimmed, *current).map(|mode| (path.clone(), mode))
                })
                .collect();
            let plan = match plan {
                Ok(plan) => plan,
                Err(message) => {
                    self.surface_error(format!("Bad chmod input: {message}"), cx);
                    return;
                }
            };
            let fs = self.fs.clone();
            cx.spawn_in(window, async move |this, cx| {
                let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
                for (path, mode) in plan {
                    if let Err(e) = fs.set_permissions(&path, mode).await {
                        failures.push((path, e));
                    }
                }
                this.update_in(cx, |this, window, cx| {
                    for (path, e) in &failures {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());
                        this.surface_error(format!("chmod {name}: {e}"), cx);
                    }
                    this.reload_entries(window, cx);
                })
                .ok();
            })
            .detach();
        }
    }

    fn execute_bulk_rename(
        &mut self,
        pattern: String,
        targets: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pattern.is_empty() || targets.is_empty() {
            return;
        }
        let fs = self.fs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
            for (index, source) in targets.iter().enumerate() {
                let parent = source.parent().unwrap_or(Path::new("/")).to_path_buf();
                let new_name = apply_rename_pattern(&pattern, index + 1);
                let destination = parent.join(&new_name);
                if destination == *source {
                    continue;
                }
                if destination.exists() {
                    failures.push((
                        source.clone(),
                        anyhow::anyhow!("target {new_name} already exists"),
                    ));
                    continue;
                }
                let result = fs
                    .rename(
                        source,
                        &destination,
                        RenameOptions {
                            overwrite: false,
                            ignore_if_exists: false,
                            create_parents: false,
                        },
                    )
                    .await;
                if let Err(e) = result {
                    failures.push((source.clone(), e));
                }
            }
            this.update_in(cx, |this, window, cx| {
                for (path, e) in &failures {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    this.surface_error(format!("Couldn't rename {name}: {e}"), cx);
                }
                this.reload_entries(window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// `p` (Normal mode): for each path in the FM clipboard, place it into
    /// the current directory, generating a numbered suffix when the
    /// destination already exists. Yank entries are copied; cut entries
    /// are renamed. Cut clears the clipboard once a paste succeeds; yank
    /// is preserved so users can paste the same set repeatedly.
    fn paste_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_paste(window, cx, false);
    }

    /// `P` (Normal mode): same as `paste_clipboard`, but if any
    /// destination already exists, prompt the user once before
    /// overwriting the whole batch.
    fn paste_clipboard_overwrite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_paste(window, cx, true);
    }

    fn start_paste(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        prompt_on_conflict: bool,
    ) {
        if self.clipboard.is_empty() {
            self.surface_error("Clipboard is empty", cx);
            return;
        }

        let is_cut = matches!(self.clipboard, FmClipboard::Cut(_));
        let sources: Vec<PathBuf> = self.clipboard.paths().to_vec();
        let destination_dir = self.current_dir.clone();
        let mut used: Vec<PathBuf> = Vec::with_capacity(sources.len());
        let mut plan: Vec<PasteEntry> = Vec::with_capacity(sources.len());

        for source in sources {
            let Some(file_name) = source.file_name() else {
                continue;
            };
            let initial = destination_dir.join(file_name);
            let initial_exists = initial.exists();

            let destination = if initial_exists && !prompt_on_conflict {
                next_available_path(&destination_dir, file_name, &used)
            } else {
                initial
            };
            used.push(destination.clone());
            plan.push(PasteEntry {
                source,
                destination,
                destination_exists: initial_exists,
            });
        }

        if plan.is_empty() {
            return;
        }

        if prompt_on_conflict && plan.iter().any(|e| e.destination_exists) {
            self.mode = PaneMode::Insert;
            self.pending_input = Some(PendingInput::ConfirmOverwrite { plan, is_cut });
            cx.notify();
            return;
        }

        self.execute_paste(plan, is_cut, /* allow_overwrite */ false, window, cx);
    }

    pub(crate) fn execute_paste(
        &mut self,
        plan: Vec<PasteEntry>,
        is_cut: bool,
        allow_overwrite: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fs = self.fs.clone();

        cx.spawn_in(window, async move |this, cx| {
            let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
            for entry in &plan {
                let result = if is_cut {
                    fs.rename(
                        &entry.source,
                        &entry.destination,
                        RenameOptions {
                            overwrite: allow_overwrite,
                            ignore_if_exists: false,
                            create_parents: false,
                        },
                    )
                    .await
                } else {
                    copy_path(
                        fs.as_ref(),
                        &entry.source,
                        &entry.destination,
                        allow_overwrite,
                    )
                    .await
                };
                if let Err(e) = result {
                    failures.push((entry.source.clone(), e));
                }
            }

            this.update_in(cx, |this, window, cx| {
                for (path, e) in &failures {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    let verb = if is_cut { "move" } else { "copy" };
                    this.surface_error(format!("Couldn't {verb} {name}: {e}"), cx);
                }
                if is_cut && failures.is_empty() {
                    this.clipboard = FmClipboard::Empty;
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
                // Esc on a find prompt restores the cursor — spec says
                // "Esc cancels; nothing changes."
                let find_origin = match pending {
                    PendingInput::FindForward { origin_index, .. }
                    | PendingInput::FindBackward { origin_index, .. } => Some(*origin_index),
                    _ => None,
                };
                self.pending_input = None;
                self.mode = PaneMode::Normal;
                if was_filter {
                    self.clear_filter();
                }
                if let Some(origin) = find_origin {
                    self.selected_index = cmp::min(
                        origin,
                        self.entries.len().saturating_sub(1),
                    );
                    self.ensure_visible();
                    self.update_preview_sync();
                }
                cx.notify();
            }
            "backspace" => {
                let find_step: Option<(String, usize, bool)> = match pending {
                    PendingInput::CreateFile(s)
                    | PendingInput::CreateDirectory(s)
                    | PendingInput::Rename { new_name: s, .. }
                    | PendingInput::BulkRename { pattern: s, .. }
                    | PendingInput::GotoPath { query: s }
                    | PendingInput::Chmod { input: s, .. } => {
                        s.pop();
                        None
                    }
                    PendingInput::Filter => {
                        self.filter_query.pop();
                        self.apply_filter();
                        None
                    }
                    PendingInput::FindForward { query, origin_index } => {
                        query.pop();
                        Some((query.clone(), *origin_index, true))
                    }
                    PendingInput::FindBackward { query, origin_index } => {
                        query.pop();
                        Some((query.clone(), *origin_index, false))
                    }
                    PendingInput::ConfirmOverwrite { .. }
                    | PendingInput::ConfirmDeleteMarked { .. } => {
                        // Nothing to edit on the prompt; expected response is
                        // y/n or Esc.
                        None
                    }
                };
                if let Some((q, origin, forward)) = find_step {
                    self.apply_find_preview(&q, origin, forward);
                }
                cx.notify();
            }
            "tab" => {
                if let Some(PendingInput::GotoPath { query }) = self.pending_input.as_mut() {
                    let extended = extend_goto_completion(query, &self.current_dir);
                    if extended != *query {
                        *query = extended;
                        cx.notify();
                    }
                }
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
                    PendingInput::ConfirmOverwrite { .. }
                    | PendingInput::ConfirmDeleteMarked { .. } => {
                        // Bare Enter on a destructive prompt is treated as
                        // "no" — safer default than acting without an
                        // explicit `y`.
                        self.mode = PaneMode::Normal;
                        cx.notify();
                    }
                    PendingInput::BulkRename { pattern, targets } if !pattern.is_empty() => {
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        self.execute_bulk_rename(pattern, targets, window, cx);
                    }
                    PendingInput::GotoPath { query } if !query.trim().is_empty() => {
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        self.goto_path(&query, window, cx);
                    }
                    PendingInput::Chmod { input, targets } if !input.trim().is_empty() => {
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        self.execute_bulk_chmod(input, targets, window, cx);
                    }
                    PendingInput::FindForward { query, .. }
                    | PendingInput::FindBackward { query, .. } => {
                        if !query.is_empty() {
                            self.last_find_pattern = Some(query);
                        }
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
                    if matches!(
                        pending,
                        PendingInput::ConfirmOverwrite { .. }
                            | PendingInput::ConfirmDeleteMarked { .. }
                    ) {
                        match ch {
                            "y" | "Y" => match self.pending_input.take() {
                                Some(PendingInput::ConfirmOverwrite { plan, is_cut }) => {
                                    self.mode = PaneMode::Normal;
                                    self.execute_paste(plan, is_cut, true, window, cx);
                                }
                                Some(PendingInput::ConfirmDeleteMarked { targets }) => {
                                    self.mode = PaneMode::Normal;
                                    self.execute_delete(targets, window, cx);
                                }
                                other => {
                                    self.pending_input = other;
                                }
                            },
                            "n" | "N" => {
                                self.pending_input = None;
                                self.mode = PaneMode::Normal;
                                cx.notify();
                            }
                            _ => {}
                        }
                    } else if ch.len() == 1 {
                        let find_step: Option<(String, usize, bool)> = match pending {
                            PendingInput::CreateFile(s)
                            | PendingInput::CreateDirectory(s)
                            | PendingInput::Rename { new_name: s, .. }
                            | PendingInput::BulkRename { pattern: s, .. }
                            | PendingInput::GotoPath { query: s }
                            | PendingInput::Chmod { input: s, .. } => {
                                s.push_str(ch);
                                None
                            }
                            PendingInput::Filter => {
                                self.filter_query.push_str(ch);
                                self.apply_filter();
                                None
                            }
                            PendingInput::FindForward { query, origin_index } => {
                                query.push_str(ch);
                                Some((query.clone(), *origin_index, true))
                            }
                            PendingInput::FindBackward { query, origin_index } => {
                                query.push_str(ch);
                                Some((query.clone(), *origin_index, false))
                            }
                            PendingInput::ConfirmOverwrite { .. }
                            | PendingInput::ConfirmDeleteMarked { .. } => {
                                // Handled in the branch above.
                                None
                            }
                        };
                        if let Some((q, origin, forward)) = find_step {
                            self.apply_find_preview(&q, origin, forward);
                        }
                        cx.notify();
                    }
                }
            }
        }
        cx.stop_propagation();
    }

    pub(crate) fn handle_key_down(
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

        if let Some(chord) = self.pending_chord.take() {
            match chord {
                'u' if !shift && !ctrl && key == "v" => {
                    self.clear_marks(cx);
                    cx.stop_propagation();
                    return;
                }
                'm' | '\'' if key != "escape" => {
                    let letter = event
                        .keystroke
                        .key_char
                        .as_deref()
                        .and_then(|s| s.chars().next())
                        .filter(|c| c.is_ascii_alphabetic());
                    if let Some(letter) = letter {
                        match chord {
                            'm' => self.save_bookmark(letter, cx),
                            '\'' => self.jump_bookmark(letter, window, cx),
                            _ => unreachable!(),
                        }
                    }
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                ',' if key != "escape" => {
                    self.handle_sort_chord(key, shift, ctrl, window, cx);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                'z' if !shift && !ctrl && key == "g" => {
                    self.toggle_gitignored(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                'c' if !shift && !ctrl && key == "m" => {
                    self.start_bulk_chmod(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                'c' if !shift && !ctrl && key == "w" => {
                    self.start_bulk_rename_editor(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        if self.visual_anchor.is_some() {
            let extends = matches!(key, "j" | "k") && !shift && !ctrl;
            let commits = matches!(key, "escape" | "enter" | "\n");
            if !extends && !commits {
                self.visual_anchor = None;
            }
        }

        let handled = match key {
            // Navigation
            "j" if !shift && !ctrl => { self.navigate_down(&NavigateDown, window, cx); true }
            "k" if !shift && !ctrl => { self.navigate_up(&NavigateUp, window, cx); true }
            "l" if !shift && !ctrl => { self.enter_directory(&EnterDirectory, window, cx); true }
            // Enter while sweeping a visual range commits the sweep
            // instead of opening the focused entry. That mirrors helix
            // and yazi behavior — the user just selected a range and
            // wouldn't expect Enter to drop them into the file.
            "enter" | "\n" if self.visual_anchor.is_some() => {
                self.commit_visual_range(cx);
                true
            }
            "enter" | "\n" => { self.enter_directory(&EnterDirectory, window, cx); true }
            "h" if !shift && !ctrl => { self.parent_directory(&ParentDirectory, window, cx); true }
            "g" if shift => { self.go_to_bottom(&GoToBottom, window, cx); true }
            "g" if !shift => { self.go_to_top(&GoToTop, window, cx); true }
            // Scrolling
            "d" if ctrl => { self.half_page_down(window, cx); true }
            "u" if ctrl => { self.half_page_up(window, cx); true }
            // Chord starter: bare `u` parks the next key for the `uv`
            // (clear-marks) chord. Subsequent two-key chords (vim-style
            // bookmarks etc.) can hang off the same slot.
            "u" if !shift && !ctrl => {
                self.pending_chord = Some('u');
                true
            }
            "pagedown" => { self.page_down(window, cx); true }
            "pageup" => { self.page_up(window, cx); true }
            // History stack — browser-style back/forward.
            "[" if !ctrl => { self.history_back(window, cx); true }
            "]" if !ctrl => { self.history_forward(window, cx); true }
            "o" if ctrl => { self.history_back(window, cx); true }
            "i" if ctrl => { self.history_forward(window, cx); true }
            // Bookmarks: vi-style two-key chords. `m<letter>` saves
            // `current_dir`; `'<letter>` jumps. Resolved on the next
            // keystroke via `pending_chord`.
            "m" if !shift && !ctrl => {
                self.pending_chord = Some('m');
                cx.notify();
                true
            }
            "'" if !ctrl => {
                self.pending_chord = Some('\'');
                cx.notify();
                true
            }
            "," if !shift && !ctrl => {
                self.pending_chord = Some(',');
                cx.notify();
                true
            }
            "z" if !shift && !ctrl => {
                self.pending_chord = Some('z');
                cx.notify();
                true
            }
            // `c` chord starter: `cm` opens the bulk-chmod input,
            // `cw` opens the bulk-rename buffer in the workspace.
            "c" if !shift && !ctrl => {
                self.pending_chord = Some('c');
                cx.notify();
                true
            }
            // `M` (shift-m) cycles the per-entry metadata column.
            "m" if shift && !ctrl => { self.cycle_line_mode(cx); true }
            // `<` / `>` resize the preview column.
            "," if shift && !ctrl => { self.nudge_preview_fraction(-crate::prefs::PREVIEW_FRACTION_STEP, cx); true }
            "." if shift && !ctrl => { self.nudge_preview_fraction(crate::prefs::PREVIEW_FRACTION_STEP, cx); true }
            "<" if !ctrl => { self.nudge_preview_fraction(-crate::prefs::PREVIEW_FRACTION_STEP, cx); true }
            ">" if !ctrl => { self.nudge_preview_fraction(crate::prefs::PREVIEW_FRACTION_STEP, cx); true }
            // Selection
            "v" if !shift && !ctrl => { self.toggle_mark(window, cx); true }
            // `V` (shift-v) starts visual-line selection. The anchor is
            // the cursor at entry; j/k from this point extend the
            // marked range. Esc / Enter exit the mode but keep the
            // marks. Other verbs (y / d / D / R / p) implicitly exit
            // visual mode via the housekeeping block above before they
            // consume the marked set.
            "v" if shift && !ctrl => { self.start_visual_range(cx); true }
            // File operations
            "y" if !shift => { self.yank_to_clipboard(window, cx); true }
            "y" if shift => { self.copy_path_to_os_clipboard(window, cx); true }
            // ctrl-a selects every visible entry; the unmodified `a`
            // arms below open the create-file / mkdir prompts.
            "a" if ctrl => { self.select_all_visible(cx); true }
            "a" if !shift => { self.create_file(window, cx); true }
            "a" if shift => { self.create_directory(window, cx); true }
            // Single-tap `d` marks for cut (paired with `p`/`P`). `D`
            // performs the destructive delete that used to be on `d`.
            "d" if !shift && !ctrl => { self.cut_to_clipboard(window, cx); true }
            "d" if shift && !ctrl => { self.delete_entry(window, cx); true }
            "p" if !shift => { self.paste_clipboard(window, cx); true }
            "p" if shift => { self.paste_clipboard_overwrite(window, cx); true }
            // ctrl-r inverts marks against the visible window; the
            // unmodified / shifted arms below remain single rename and
            // bulk-rename, respectively.
            "r" if ctrl => { self.invert_marks_visible(cx); true }
            "r" if !shift => { self.rename_entry(window, cx); true }
            "r" if shift => { self.start_bulk_rename(window, cx); true }
            // Toggles
            "." if !shift && !ctrl => { self.toggle_hidden(&ToggleHidden, window, cx); true }
            // Fuzzy filter (phase-7: moved from `/` to `f`; `/` is now find-forward).
            "f" if !shift && !ctrl => { self.start_filter(window, cx); true }
            // Find: yazi-style incremental jump. `/` searches forward, `?`
            // backward; on commit the query is parked in
            // `last_find_pattern` so `n` / `N` repeat the walk.
            "/" if !ctrl => { self.start_find_forward(window, cx); true }
            "?" if !ctrl => { self.start_find_backward(window, cx); true }
            "n" if !shift && !ctrl => { self.find_next(cx); true }
            "n" if shift && !ctrl => { self.find_prev(cx); true }
            "escape" if self.visual_anchor.is_some() => {
                self.commit_visual_range(cx);
                true
            }
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

    /// Resolve `query` against `current_dir` (expanding a leading `~`)
    /// and navigate. Failures surface via the existing `surface_error`
    /// toast: empty path, missing target, non-directory, or unreadable.
    pub(crate) fn goto_path(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let resolved = match resolve_goto_query(query, &self.current_dir) {
            Some(p) => p,
            None => {
                self.surface_error("Empty path", cx);
                return;
            }
        };
        if !resolved.exists() {
            self.surface_error(format!("Path does not exist: {}", resolved.display()), cx);
            return;
        }
        if !resolved.is_dir() {
            self.surface_error(format!("Not a directory: {}", resolved.display()), cx);
            return;
        }
        if std::fs::read_dir(&resolved).is_err() {
            self.surface_error(format!("Cannot read: {}", resolved.display()), cx);
            return;
        }
        if resolved == self.current_dir {
            return;
        }
        self.push_history_back(self.current_dir.clone());
        self.forward_stack.clear();
        self.current_dir = resolved;
        self.selected_index = 0;
        self.reload_entries(window, cx);
    }

    fn history_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prev) = self.back_stack.pop_back() else {
            return;
        };
        self.push_history_forward(self.current_dir.clone());
        self.current_dir = prev;
        self.selected_index = 0;
        self.reload_entries(window, cx);
    }

    fn history_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = self.forward_stack.pop_back() else {
            return;
        };
        self.push_history_back(self.current_dir.clone());
        self.current_dir = next;
        self.selected_index = 0;
        self.reload_entries(window, cx);
    }

    fn save_bookmark(&mut self, letter: char, cx: &mut Context<Self>) {
        let dir = self.current_dir.clone();
        let displayed = dir.display().to_string();
        cx.update_global::<crate::bookmarks::BookmarkStore, _>(|store, _| {
            store.set(letter, dir);
        });
        self.surface_error(format!("Bookmarked '{letter}' → {displayed}"), cx);
    }

    fn handle_sort_chord(
        &mut self,
        key: &str,
        shift: bool,
        _ctrl: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::prefs::SortMode;
        let mode = match (key, shift) {
            ("n", false) => Some(SortMode::Name),
            ("s", false) => Some(SortMode::Size),
            ("m", false) => Some(SortMode::Mtime),
            ("b", false) => Some(SortMode::Btime),
            ("e", false) => Some(SortMode::Extension),
            ("r", false) => Some(SortMode::Random),
            ("n", true) => Some(SortMode::Natural),
            _ => None,
        };
        if let Some(mode) = mode {
            self.apply_sort(mode, cx);
            self.reload_entries(window, cx);
            return;
        }
        if key == "," && !shift {
            self.reverse = !self.reverse;
            let value = self.reverse;
            cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_reverse(value));
            self.reload_entries(window, cx);
        }
    }

    fn apply_sort(&mut self, mode: crate::prefs::SortMode, cx: &mut Context<Self>) {
        self.sort = mode;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_sort(mode));
    }

    fn cycle_line_mode(&mut self, cx: &mut Context<Self>) {
        self.line_mode = self.line_mode.next();
        let mode = self.line_mode;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_line_mode(mode));
        cx.notify();
    }

    fn toggle_gitignored(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_gitignored = !self.show_gitignored;
        let value = self.show_gitignored;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_show_gitignored(value));
        self.reload_entries(window, cx);
    }

    fn nudge_preview_fraction(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.preview_fraction = crate::prefs::clamp_fraction(self.preview_fraction + delta);
        let value = self.preview_fraction;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_preview_fraction(value));
        cx.notify();
    }

    fn jump_bookmark(&mut self, letter: char, window: &mut Window, cx: &mut Context<Self>) {
        let target = cx
            .global::<crate::bookmarks::BookmarkStore>()
            .get(letter)
            .map(|p| p.to_path_buf());
        let Some(target) = target else {
            self.surface_error(format!("Bookmark '{letter}' is empty"), cx);
            return;
        };
        if !target.is_dir() {
            self.surface_error(
                format!("Bookmark '{letter}' → {} no longer exists", target.display()),
                cx,
            );
            return;
        }
        if target == self.current_dir {
            return;
        }
        self.push_history_back(self.current_dir.clone());
        self.forward_stack.clear();
        self.current_dir = target;
        self.selected_index = 0;
        self.reload_entries(window, cx);
    }
}

impl Focusable for FileManager {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
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

/// Find a destination filename that doesn't collide. If `foo.txt` exists,
/// try `foo (2).txt`, `foo (3).txt`, etc. `used` are destinations already
/// claimed by an in-flight paste batch so a single `p` of two files named
/// `foo.txt` doesn't try to write both to the same target.
fn next_available_path(
    directory: &Path,
    file_name: &std::ffi::OsStr,
    used: &[PathBuf],
) -> PathBuf {
    let name_str = file_name.to_string_lossy();
    let (stem, extension) = split_stem_extension(name_str.as_ref());
    let mut counter: usize = 2;
    loop {
        let candidate_name = if extension.is_empty() {
            format!("{stem} ({counter})")
        } else {
            format!("{stem} ({counter}).{extension}")
        };
        let candidate = directory.join(&candidate_name);
        if !candidate.exists() && !used.iter().any(|p| p == &candidate) {
            return candidate;
        }
        counter = counter.saturating_add(1);
        if counter > 9999 {
            // Extreme fallback: give up and return the latest candidate.
            // Callers will surface the resulting fs error.
            return candidate;
        }
    }
}

/// Split a filename into (stem, extension). Treats leading-dot files as
/// extension-less so the numbered suffix lands at the end of
/// `.gitignore` rather than mangling the hidden marker.
fn split_stem_extension(name: &str) -> (&str, &str) {
    if let Some(rest) = name.strip_prefix('.') {
        if let Some(idx) = rest.rfind('.') {
            // `.foo.bar` → stem = ".foo", ext = "bar"
            let stem_end = idx + 1;
            return (&name[..stem_end], &name[stem_end + 1..]);
        }
        return (name, "");
    }
    match name.rfind('.') {
        None => (name, ""),
        Some(idx) => (&name[..idx], &name[idx + 1..]),
    }
}

async fn copy_path(
    fs: &dyn fs::Fs,
    source: &Path,
    destination: &Path,
    allow_overwrite: bool,
) -> anyhow::Result<()> {
    let options = CopyOptions {
        overwrite: allow_overwrite,
        ignore_if_exists: false,
    };
    if fs.is_dir(source).await {
        // Mirror the source directory at `destination` first so the
        // recursive walk has somewhere to write children into.
        if allow_overwrite {
            fs.remove_dir(
                destination,
                fs::RemoveOptions {
                    recursive: true,
                    ignore_if_not_exists: true,
                },
            )
            .await
            .log_err();
        }
        fs.create_dir(destination).await?;
        copy_recursive(fs, source, destination, options).await
    } else {
        fs.copy_file(source, destination, options).await
    }
}

fn plural_y(count: usize) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

/// Substitute every `{}` in `pattern` with `index`. Multiple
/// placeholders all receive the same counter value for the row; if no
/// `{}` is present, the index is appended to the stem so the result is
/// still unique per row. The stem-append branch tries to preserve the
/// extension (e.g. `clean.png` with no `{}` and index 3 becomes
/// `clean3.png`).
fn apply_rename_pattern(pattern: &str, index: usize) -> String {
    let counter = index.to_string();
    if pattern.contains("{}") {
        return pattern.replace("{}", &counter);
    }
    let (stem, extension) = split_stem_extension(pattern);
    if extension.is_empty() {
        format!("{stem}{counter}")
    } else {
        format!("{stem}{counter}.{extension}")
    }
}

/// Resolve a chmod input string against the entry's current mode.
/// Accepts either an octal number (`755`, `0755`, `0o755`) or one or
/// more symbolic clauses separated by `,` (`u+x`, `a=r,go-w`). The
/// `current` mode is only consulted for symbolic input; pure-octal
/// input replaces the mode outright. Returns the final mode masked to
/// `0o7777` (so set-uid / set-gid bits survive) on success, or a short
/// human message describing why the input was rejected.
pub(crate) fn apply_chmod_input(input: &str, current: Option<u32>) -> Result<u32, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty input".to_string());
    }
    if let Some(mode) = parse_octal_mode(trimmed) {
        return Ok(mode & 0o7777);
    }
    let base = current.unwrap_or(0) & 0o7777;
    apply_symbolic_chmod(trimmed, base).map(|m| m & 0o7777)
}

fn parse_octal_mode(input: &str) -> Option<u32> {
    let stripped = input
        .strip_prefix("0o")
        .or_else(|| input.strip_prefix("0O"))
        .or_else(|| input.strip_prefix('0'))
        .unwrap_or(input);
    let candidate = if stripped.is_empty() { input } else { stripped };
    if candidate.is_empty() || !candidate.chars().all(|c| ('0'..='7').contains(&c)) {
        return None;
    }
    u32::from_str_radix(candidate, 8).ok()
}

/// Parse a comma-separated chmod symbolic expression and fold it over
/// `base`. Grammar (a subset of GNU chmod's): `[ugoa]*[+-=][rwx]+`.
/// Empty `who` is treated as `a` (matching chmod's default minus the
/// umask wrinkle — we apply to all three classes since this is an
/// interactive verb on already-existing files).
fn apply_symbolic_chmod(input: &str, base: u32) -> Result<u32, String> {
    let mut mode = base;
    for clause in input.split(',') {
        let clause = clause.trim();
        if clause.is_empty() {
            return Err(format!("empty clause in '{input}'"));
        }
        mode = apply_symbolic_clause(clause, mode)?;
    }
    Ok(mode)
}

fn apply_symbolic_clause(clause: &str, mode: u32) -> Result<u32, String> {
    let bytes = clause.as_bytes();
    let mut idx = 0;
    let mut who_mask: u32 = 0;
    while idx < bytes.len() {
        let bit = match bytes[idx] {
            b'u' => 0o700,
            b'g' => 0o070,
            b'o' => 0o007,
            b'a' => 0o777,
            _ => break,
        };
        who_mask |= bit;
        idx += 1;
    }
    if who_mask == 0 {
        who_mask = 0o777;
    }
    if idx >= bytes.len() {
        return Err(format!("missing operator in '{clause}'"));
    }
    let op = bytes[idx];
    if !matches!(op, b'+' | b'-' | b'=') {
        return Err(format!("expected +/-/= in '{clause}'"));
    }
    idx += 1;
    let mut perm_bits: u32 = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'r' => perm_bits |= 0o444,
            b'w' => perm_bits |= 0o222,
            b'x' => perm_bits |= 0o111,
            other => {
                return Err(format!(
                    "unsupported permission '{}' in '{clause}'",
                    other as char
                ));
            }
        }
        idx += 1;
    }
    let masked = perm_bits & who_mask;
    Ok(match op {
        b'+' => mode | masked,
        b'-' => mode & !masked,
        b'=' => (mode & !who_mask) | masked,
        _ => unreachable!(),
    })
}

/// Seed the input bar with a sensible default pattern based on the
/// first marked entry. Keeps the original extension so the user only
/// has to type the stem before submitting.
fn default_bulk_rename_pattern(first: &Path) -> String {
    let name = first
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let (_, extension) = split_stem_extension(&name);
    if extension.is_empty() {
        "{}".to_string()
    } else {
        format!("{{}}.{extension}")
    }
}


/// Options governing how a directory listing is built. `show_hidden`
/// keeps the dotfile filter; `sort` + `reverse` set the comparator;
/// `show_gitignored` is consulted by the FM (after git status is
/// populated) — `read_dir_sync` itself does not see git status.
#[derive(Clone, Copy)]
pub(crate) struct ReadDirOptions {
    pub show_hidden: bool,
    pub sort: crate::prefs::SortMode,
    pub reverse: bool,
}

impl Default for ReadDirOptions {
    fn default() -> Self {
        Self {
            show_hidden: false,
            sort: crate::prefs::SortMode::Name,
            reverse: false,
        }
    }
}

pub(crate) fn read_dir_sync(path: &Path, options: ReadDirOptions) -> Vec<DirEntry> {
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return Vec::new();
    };

    let mut entries: Vec<DirEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_hidden = name.starts_with('.');
            if !options.show_hidden && is_hidden {
                return None;
            }
            let metadata = e.metadata().ok()?;
            let file_type = e.file_type().ok()?;
            let mtime = metadata.modified().ok();
            let btime = metadata.created().ok().or(mtime);
            let (mode, uid, gid) = unix_metadata(&metadata);
            Some(DirEntry {
                name,
                path: e.path(),
                is_dir: metadata.is_dir(),
                is_hidden,
                is_symlink: file_type.is_symlink(),
                size: metadata.len(),
                git_status: None,
                mtime,
                btime,
                mode,
                uid,
                gid,
            })
        })
        .collect();

    sort_entries(&mut entries, options.sort, options.reverse);
    entries
}

#[cfg(unix)]
fn unix_metadata(metadata: &std::fs::Metadata) -> (Option<u32>, Option<u32>, Option<u32>) {
    use std::os::unix::fs::MetadataExt;
    (Some(metadata.mode()), Some(metadata.uid()), Some(metadata.gid()))
}

#[cfg(not(unix))]
fn unix_metadata(_metadata: &std::fs::Metadata) -> (Option<u32>, Option<u32>, Option<u32>) {
    (None, None, None)
}

pub(crate) fn sort_entries(
    entries: &mut [DirEntry],
    mode: crate::prefs::SortMode,
    reverse: bool,
) {
    use crate::prefs::SortMode;
    if matches!(mode, SortMode::Random) {
        // Shuffle the within-group order but keep dirs-before-files.
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        let split = entries.iter().position(|e| !e.is_dir).unwrap_or(entries.len());
        let (dirs, files) = entries.split_at_mut(split);
        dirs.shuffle(&mut rng);
        files.shuffle(&mut rng);
        if reverse {
            dirs.reverse();
            files.reverse();
        }
        return;
    }

    entries.sort_by(|a, b| {
        let group = b.is_dir.cmp(&a.is_dir);
        if group != cmp::Ordering::Equal {
            return group;
        }
        let within = compare_in_group(a, b, mode);
        if reverse {
            within.reverse()
        } else {
            within
        }
    });
}

fn compare_in_group(a: &DirEntry, b: &DirEntry, mode: crate::prefs::SortMode) -> cmp::Ordering {
    use crate::prefs::SortMode;
    let by_name = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
    match mode {
        SortMode::Name => by_name(),
        SortMode::Size => a.size.cmp(&b.size).then_with(by_name),
        SortMode::Mtime => a.mtime.cmp(&b.mtime).then_with(by_name),
        SortMode::Btime => a.btime.cmp(&b.btime).then_with(by_name),
        SortMode::Extension => {
            let ea = extension_key(&a.name);
            let eb = extension_key(&b.name);
            ea.cmp(&eb).then_with(by_name)
        }
        SortMode::Natural => natural_compare(&a.name, &b.name),
        SortMode::Random => cmp::Ordering::Equal,
    }
}

fn extension_key(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Numeric-aware string comparison: contiguous ASCII-digit runs compare
/// by integer value; everything else is case-insensitive byte-wise.
/// Keeps `file2 < file10` while `apple` still beats `banana`.
pub(crate) fn natural_compare(a: &str, b: &str) -> cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return cmp::Ordering::Equal,
            (None, Some(_)) => return cmp::Ordering::Less,
            (Some(_), None) => return cmp::Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na: String = take_while(&mut ai, |c| c.is_ascii_digit());
                    let nb: String = take_while(&mut bi, |c| c.is_ascii_digit());
                    let va: u128 = na.parse().unwrap_or(0);
                    let vb: u128 = nb.parse().unwrap_or(0);
                    match va.cmp(&vb) {
                        cmp::Ordering::Equal => continue,
                        other => return other,
                    }
                } else {
                    let la = ca.to_ascii_lowercase();
                    let lb = cb.to_ascii_lowercase();
                    match la.cmp(&lb) {
                        cmp::Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        other => return other,
                    }
                }
            }
        }
    }
}

fn take_while<I: Iterator<Item = char>, F: Fn(char) -> bool>(
    iter: &mut std::iter::Peekable<I>,
    pred: F,
) -> String {
    let mut out = String::new();
    while let Some(&c) = iter.peek() {
        if !pred(c) {
            break;
        }
        out.push(c);
        iter.next();
    }
    out
}

/// Maximum number of bytes shown in the hex/ASCII dump for the binary
/// preview fallback. Sized to fit 16 lines of 16 bytes each.
pub(crate) const BINARY_HEAD_BYTES: usize = 256;

/// Build the metadata + first-bytes snapshot for the binary preview. The
/// read failure path (permissions, vanishing entry) returns an empty
/// `head` — the header line still renders so the user sees the name,
/// size, and mime even when the bytes themselves are unreadable.
pub(crate) fn read_binary_info(path: &Path, name: String, size: u64) -> BinaryInfo {
    let mime = guess_mime(path);
    let head = read_head_bytes(path, BINARY_HEAD_BYTES);
    BinaryInfo {
        name,
        size,
        mime,
        head,
    }
}

fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first()
        .map(|m| m.essence_str().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn read_head_bytes(path: &Path, max: usize) -> Vec<u8> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buf = vec![0_u8; max];
    match file.read(&mut buf) {
        Ok(read) => {
            buf.truncate(read);
            buf
        }
        Err(_) => Vec::new(),
    }
}

/// Format `bytes` as `xxd`-style lines: 8-byte-aligned offset, hex pairs
/// in two groups of eight separated by a wide gap, then the ASCII
/// rendering (printable bytes verbatim, non-printable replaced with
/// `.`). Each line covers up to 16 bytes; the final line is padded so
/// the ASCII column lines up.
pub(crate) fn format_hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (chunk_index, chunk) in bytes.chunks(16).enumerate() {
        let offset = chunk_index * 16;
        out.push_str(&format!("{offset:08x}  "));

        for i in 0..16 {
            if let Some(byte) = chunk.get(i) {
                out.push_str(&format!("{byte:02x} "));
            } else {
                out.push_str("   ");
            }
            if i == 7 {
                out.push(' ');
            }
        }

        out.push(' ');
        out.push('|');
        for byte in chunk {
            let c = *byte;
            if (0x20..=0x7e).contains(&c) {
                out.push(c as char);
            } else {
                out.push('.');
            }
        }
        for _ in chunk.len()..16 {
            out.push(' ');
        }
        out.push('|');
        out.push('\n');
    }
    out
}

/// File extensions the FM offers to render as images. Matches the
/// formats Zed's vendored `gpui::img` supports out of the box.
pub(crate) const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico",
];

/// Returns true when `path`'s extension is in [`IMAGE_EXTENSIONS`].
pub(crate) fn is_image_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let lower = ext.to_ascii_lowercase();
    IMAGE_EXTENSIONS.iter().any(|known| *known == lower)
}

/// Snapshot metadata for the image preview. Dimensions come from
/// `image::image_dimensions`, which only parses the header — orders of
/// magnitude cheaper than decoding the pixel data. A failure leaves
/// `dimensions` as `None` and the view falls back to a metadata
/// placeholder.
pub(crate) fn read_image_info(path: &Path, name: String, size: u64) -> ImageInfo {
    let dimensions = image::image_dimensions(path).ok();
    let mime = guess_mime(path);
    ImageInfo {
        name,
        path: path.to_path_buf(),
        size,
        mime,
        dimensions,
    }
}

/// Upper bound on the number of archive entries surfaced in the
/// preview. Anything beyond gets summarised as a `… N more` line.
pub(crate) const ARCHIVE_ENTRIES_CAP: usize = 200;

/// Returns true when `path`'s extension marks it as one of the archive
/// formats the preview pane can list. Multi-dot variants (`.tar.gz`,
/// `.tar.bz2`) are detected explicitly because `Path::extension` only
/// returns the rightmost segment.
pub(crate) fn is_archive_path(path: &Path) -> bool {
    archive_kind(path).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
}

fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    let lower = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Some(ArchiveKind::TarGz);
    }
    if lower.ends_with(".tar") {
        return Some(ArchiveKind::Tar);
    }
    if lower.ends_with(".zip") {
        return Some(ArchiveKind::Zip);
    }
    None
}

/// Open `path` as an archive and collect its entries up to the cap. The
/// returned listing distinguishes "entries we kept" from "extra entries
/// we omitted" so the view can show a `… N more` line.
pub(crate) fn read_archive_listing(path: &Path) -> Option<ArchiveListing> {
    let kind = archive_kind(path)?;
    match kind {
        ArchiveKind::Zip => read_zip_listing(path),
        ArchiveKind::Tar => read_tar_listing(path),
        ArchiveKind::TarGz => read_tar_gz_listing(path),
    }
}

fn read_zip_listing(path: &Path) -> Option<ArchiveListing> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let total = archive.len();
    let mut entries = Vec::with_capacity(total.min(ARCHIVE_ENTRIES_CAP));
    for index in 0..total.min(ARCHIVE_ENTRIES_CAP) {
        let Ok(file) = archive.by_index(index) else {
            break;
        };
        entries.push(ArchiveEntry {
            name: file.name().to_string(),
            size: Some(file.size()),
        });
    }
    let extra = total.saturating_sub(entries.len());
    Some(ArchiveListing { entries, extra })
}

fn read_tar_listing(path: &Path) -> Option<ArchiveListing> {
    let file = std::fs::File::open(path).ok()?;
    collect_tar_entries(tar::Archive::new(file))
}

fn read_tar_gz_listing(path: &Path) -> Option<ArchiveListing> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = flate2::read::GzDecoder::new(file);
    collect_tar_entries(tar::Archive::new(decoder))
}

fn collect_tar_entries<R: std::io::Read>(mut archive: tar::Archive<R>) -> Option<ArchiveListing> {
    let iter = archive.entries().ok()?;
    let mut entries = Vec::new();
    let mut extra = 0_usize;
    for entry in iter {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(path) = entry.path() else { continue };
        let name = path.to_string_lossy().to_string();
        let size = entry.header().size().ok();
        if entries.len() < ARCHIVE_ENTRIES_CAP {
            entries.push(ArchiveEntry { name, size });
        } else {
            extra += 1;
        }
    }
    Some(ArchiveListing { entries, extra })
}

/// Resolve a user-typed goto query against `current_dir`. Returns `None`
/// for an empty input. Expands a leading `~` to `$HOME` and resolves
/// relative paths against `current_dir`.
pub(crate) fn resolve_goto_query(query: &str, current_dir: &Path) -> Option<PathBuf> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let expanded = expand_tilde(trimmed);
    let candidate = if expanded.is_absolute() {
        expanded
    } else {
        current_dir.join(expanded)
    };
    Some(candidate)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    } else if path == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    PathBuf::from(path)
}

/// Extend `query` with the longest-common-prefix of filesystem entries
/// that start with the typed leaf. Splits `query` at the rightmost `/`
/// and reads the directory portion against `current_dir`. Appends a
/// trailing `/` when exactly one candidate matches and it's a directory.
pub(crate) fn extend_goto_completion(query: &str, current_dir: &Path) -> String {
    let (dir_part, leaf) = split_dir_leaf(query);
    let base =
        resolve_goto_query(dir_part, current_dir).unwrap_or_else(|| current_dir.to_path_buf());
    let Ok(entries) = std::fs::read_dir(&base) else {
        return query.to_string();
    };
    let candidates: Vec<(String, bool)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(leaf) {
                return None;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some((name, is_dir))
        })
        .collect();
    if candidates.is_empty() {
        return query.to_string();
    }
    let lcp = longest_common_prefix(candidates.iter().map(|(n, _)| n.as_str()));
    let completed = if candidates.len() == 1 && candidates[0].1 {
        format!("{}/", candidates[0].0)
    } else {
        lcp
    };
    if completed.len() <= leaf.len() {
        return query.to_string();
    }
    if dir_part.is_empty() {
        completed
    } else if dir_part.ends_with('/') {
        format!("{dir_part}{completed}")
    } else {
        format!("{dir_part}/{completed}")
    }
}

fn split_dir_leaf(query: &str) -> (&str, &str) {
    match query.rfind('/') {
        Some(ix) => (&query[..=ix], &query[ix + 1..]),
        None => ("", query),
    }
}

fn longest_common_prefix<'a, I>(items: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut iter = items.into_iter();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for s in iter {
        let mut new_len = 0;
        for (a, b) in prefix.iter().zip(s.chars()) {
            if *a == b {
                new_len += 1;
            } else {
                break;
            }
        }
        prefix.truncate(new_len);
        if prefix.is_empty() {
            return String::new();
        }
    }
    prefix.into_iter().collect()
}

pub fn init(cx: &mut App) {
    crate::bookmarks::init(cx);
    crate::prefs::init(cx);
    crate::goto_completer::register();
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
    registry.register::<Paste>(&[]);
    registry.register::<PasteOverwrite>(&[]);

    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &Open, window, cx| {
            open_file_manager(workspace, window, cx);
        });
        workspace.register_action(
            |workspace, _: &zed_actions::file_manager::OpenFileManager, window, cx| {
                open_file_manager(workspace, window, cx);
            },
        );
        workspace.register_action(|workspace, action: &Reveal, window, cx| {
            handle_reveal(workspace, action.0.clone(), window, cx);
        });
        workspace.register_action(|workspace, action: &GotoPath, window, cx| {
            let seed = action.0.clone();
            focus_or_open_fm_then(workspace, window, cx, |fm, window, cx| {
                fm.start_goto_path(seed, window, cx);
            });
        });
    })
    .detach();
}

fn focus_or_open_fm_then<F>(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    body: F,
) where
    F: FnOnce(&mut FileManager, &mut Window, &mut Context<FileManager>),
{
    if let Some(item) = workspace.recent_active_item_by_type::<FileManager>(cx) {
        workspace.activate_item(&item, true, true, window, cx);
        item.update(cx, |fm, cx| body(fm, window, cx));
        return;
    }
    open_file_manager(workspace, window, cx);
    if let Some(item) = workspace.recent_active_item_by_type::<FileManager>(cx) {
        item.update(cx, |fm, cx| body(fm, window, cx));
    }
}

fn handle_reveal(
    workspace: &mut Workspace,
    path: PathBuf,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if path.as_os_str().is_empty() {
        return;
    }
    let target_dir = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| path.clone());
    let select_name = path.file_name().map(|n| n.to_string_lossy().to_string());

    if let Some(item) = workspace.recent_active_item_by_type::<FileManager>(cx) {
        workspace.activate_item(&item, true, true, window, cx);
        item.update(cx, |fm, cx| {
            fm.reveal_path(target_dir, select_name, window, cx)
        });
        return;
    }

    let fs = workspace.app_state().fs.clone();
    let weak_workspace = workspace.weak_handle();
    let file_manager =
        cx.new(|cx| FileManager::new(target_dir.clone(), weak_workspace, fs, window, cx));
    if let Some(name) = select_name {
        file_manager.update(cx, |fm, cx| fm.select_entry_by_name(&name, cx));
    }
    workspace.add_item_to_active_pane(Box::new(file_manager), None, true, window, cx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_tree(layout: &[(&str, bool)]) -> TempDir {
        let dir = TempDir::new().expect("create tempdir");
        for (name, is_dir) in layout {
            let p = dir.path().join(name);
            if *is_dir {
                fs::create_dir(&p).expect("mkdir");
            } else {
                fs::write(&p, b"").expect("touch");
            }
        }
        dir
    }

    #[test]
    fn read_dir_sync_filters_hidden_when_show_hidden_false() {
        let dir = make_tree(&[
            ("visible.txt", false),
            (".hidden.txt", false),
            ("subdir", true),
            (".dotdir", true),
        ]);
        let entries = read_dir_sync(dir.path(), ReadDirOptions::default());
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["subdir", "visible.txt"]);
    }

    #[test]
    fn read_dir_sync_includes_hidden_when_show_hidden_true() {
        let dir = make_tree(&[
            ("visible.txt", false),
            (".hidden.txt", false),
            ("subdir", true),
            (".dotdir", true),
        ]);
        let entries = read_dir_sync(
            dir.path(),
            ReadDirOptions {
                show_hidden: true,
                ..ReadDirOptions::default()
            },
        );
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // Directories first, then files; each group case-insensitive ascending.
        assert_eq!(names, vec![".dotdir", "subdir", ".hidden.txt", "visible.txt"]);
        let hidden_flags: Vec<bool> = entries.iter().map(|e| e.is_hidden).collect();
        assert_eq!(hidden_flags, vec![true, false, true, false]);
    }

    #[test]
    fn read_dir_sync_sorts_dirs_before_files() {
        let dir = make_tree(&[
            ("zfile.txt", false),
            ("adir", true),
            ("bdir", true),
            ("afile.txt", false),
        ]);
        let entries = read_dir_sync(dir.path(), ReadDirOptions::default());
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["adir", "bdir", "afile.txt", "zfile.txt"]);
    }

    #[test]
    fn read_dir_sync_sort_is_case_insensitive() {
        let dir = make_tree(&[
            ("Zebra.txt", false),
            ("apple.txt", false),
            ("Banana.txt", false),
        ]);
        let entries = read_dir_sync(dir.path(), ReadDirOptions::default());
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["apple.txt", "Banana.txt", "Zebra.txt"]);
    }

    #[test]
    fn read_dir_sync_unreadable_path_returns_empty() {
        let entries = read_dir_sync(
            Path::new("/nonexistent/path/that/does/not/exist"),
            ReadDirOptions::default(),
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn is_subsequence_empty_needle_matches_anything() {
        assert!(is_subsequence(&[], "anything"));
        assert!(is_subsequence(&[], ""));
    }

    #[test]
    fn is_subsequence_matches_contiguous() {
        let needle: Vec<char> = "foo".chars().collect();
        assert!(is_subsequence(&needle, "foobar"));
        assert!(is_subsequence(&needle, "barfoo"));
    }

    #[test]
    fn is_subsequence_matches_non_contiguous() {
        let needle: Vec<char> = "fb".chars().collect();
        assert!(is_subsequence(&needle, "foobar"));
    }

    #[test]
    fn is_subsequence_case_insensitive_on_haystack() {
        // Implementation lowercases haystack chars; needle is assumed
        // lowercase by the caller (apply_filter does that).
        let needle: Vec<char> = "foo".chars().collect();
        assert!(is_subsequence(&needle, "FOOBAR"));
    }

    #[test]
    fn is_subsequence_no_match() {
        let needle: Vec<char> = "xyz".chars().collect();
        assert!(!is_subsequence(&needle, "foobar"));
    }

    #[test]
    fn split_stem_extension_simple_file() {
        assert_eq!(split_stem_extension("foo.txt"), ("foo", "txt"));
    }

    #[test]
    fn split_stem_extension_no_extension() {
        assert_eq!(split_stem_extension("README"), ("README", ""));
    }

    #[test]
    fn split_stem_extension_multiple_dots() {
        // Only the rightmost dot is the extension boundary.
        assert_eq!(split_stem_extension("foo.tar.gz"), ("foo.tar", "gz"));
    }

    #[test]
    fn split_stem_extension_dotfile_no_extension() {
        // `.gitignore` is the whole stem; suffix lands after the name.
        assert_eq!(split_stem_extension(".gitignore"), (".gitignore", ""));
    }

    #[test]
    fn split_stem_extension_dotfile_with_extension() {
        // `.foo.bar` → stem keeps the leading dot.
        assert_eq!(split_stem_extension(".foo.bar"), (".foo", "bar"));
    }

    #[test]
    fn next_available_path_uses_collision_free_initial_when_unused() {
        let dir = TempDir::new().expect("create tempdir");
        let p = next_available_path(dir.path(), std::ffi::OsStr::new("brand_new.txt"), &[]);
        // The function only generates numbered suffixes — it always picks
        // a "(N)" variant. The initial collision-free case is handled by
        // the caller (start_paste) before invoking this helper.
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "brand_new (2).txt");
    }

    #[test]
    fn next_available_path_finds_first_free_slot() {
        let dir = TempDir::new().expect("create tempdir");
        fs::write(dir.path().join("foo.txt"), b"").expect("touch");
        fs::write(dir.path().join("foo (2).txt"), b"").expect("touch");
        let p = next_available_path(dir.path(), std::ffi::OsStr::new("foo.txt"), &[]);
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "foo (3).txt");
    }

    #[test]
    fn next_available_path_respects_used_paths() {
        let dir = TempDir::new().expect("create tempdir");
        let already = dir.path().join("foo (2).txt");
        let p = next_available_path(
            dir.path(),
            std::ffi::OsStr::new("foo.txt"),
            std::slice::from_ref(&already),
        );
        // (2) is reserved by the in-flight batch even though it doesn't
        // exist on disk yet.
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "foo (3).txt");
    }

    #[test]
    fn apply_rename_pattern_substitutes_single_placeholder() {
        assert_eq!(
            apply_rename_pattern("screenshot-{}.png", 1),
            "screenshot-1.png"
        );
        assert_eq!(
            apply_rename_pattern("screenshot-{}.png", 42),
            "screenshot-42.png"
        );
    }

    #[test]
    fn apply_rename_pattern_substitutes_every_placeholder() {
        // Both `{}` get the same counter — keeps the rule trivially
        // predictable.
        assert_eq!(apply_rename_pattern("{}-{}.txt", 7), "7-7.txt");
    }

    #[test]
    fn apply_rename_pattern_appends_when_no_placeholder() {
        // Without `{}` the index appends before the extension so the
        // rename is still unique per row.
        assert_eq!(apply_rename_pattern("clean.png", 3), "clean3.png");
        assert_eq!(apply_rename_pattern("README", 5), "README5");
    }

    #[test]
    fn default_bulk_rename_pattern_preserves_extension() {
        let p = Path::new("/tmp/foo.png");
        assert_eq!(default_bulk_rename_pattern(p), "{}.png");
    }

    #[test]
    fn default_bulk_rename_pattern_extensionless() {
        let p = Path::new("/tmp/README");
        assert_eq!(default_bulk_rename_pattern(p), "{}");
    }

    #[test]
    fn next_available_path_handles_extensionless_name() {
        let dir = TempDir::new().expect("create tempdir");
        fs::write(dir.path().join("README"), b"").expect("touch");
        let p = next_available_path(dir.path(), std::ffi::OsStr::new("README"), &[]);
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "README (2)");
    }

    #[test]
    fn format_hex_dump_one_full_line() {
        let bytes = [
            0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        let out = format_hex_dump(&bytes);
        assert_eq!(
            out,
            "00000000  7f 45 4c 46 02 01 01 00  00 00 00 00 00 00 00 00  |.ELF............|\n"
        );
    }

    #[test]
    fn format_hex_dump_partial_last_line_pads_ascii_column() {
        let bytes = [0x41, 0x42, 0x43];
        let out = format_hex_dump(&bytes);
        assert_eq!(
            out,
            "00000000  41 42 43                                          |ABC             |\n"
        );
    }

    #[test]
    fn format_hex_dump_empty_input_is_empty_string() {
        assert_eq!(format_hex_dump(&[]), "");
    }

    #[test]
    fn format_hex_dump_replaces_non_printable_with_dot() {
        let bytes = [0x00, 0x09, 0x0a, 0x20, 0x7e, 0x7f, 0xff];
        let out = format_hex_dump(&bytes);
        assert!(out.contains("|... ~..         |"), "got: {out}");
    }

    #[test]
    fn format_hex_dump_offset_increments_by_16() {
        let bytes = vec![0u8; 17];
        let out = format_hex_dump(&bytes);
        assert!(out.starts_with("00000000  "));
        assert!(out.contains("\n00000010  "));
    }

    #[test]
    fn read_binary_info_truncates_at_256_bytes() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("blob.bin");
        let payload: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
        fs::write(&path, &payload).expect("write");
        let info = read_binary_info(&path, "blob.bin".to_string(), payload.len() as u64);
        assert_eq!(info.head.len(), BINARY_HEAD_BYTES);
        assert_eq!(info.name, "blob.bin");
        assert_eq!(info.size, 512);
    }

    #[test]
    fn read_binary_info_missing_path_returns_empty_head() {
        let info = read_binary_info(Path::new("/nonexistent/blob.bin"), "blob.bin".into(), 0);
        assert!(info.head.is_empty());
        assert_eq!(info.mime, "application/octet-stream");
    }

    #[test]
    fn read_binary_info_uses_extension_mime_when_known() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("a.png");
        fs::write(&path, b"\x89PNG\r\n\x1a\n").expect("write");
        let info = read_binary_info(&path, "a.png".into(), 8);
        assert_eq!(info.mime, "image/png");
    }

    #[test]
    fn is_archive_path_recognises_known_extensions() {
        assert!(is_archive_path(Path::new("foo.zip")));
        assert!(is_archive_path(Path::new("foo.tar")));
        assert!(is_archive_path(Path::new("foo.tar.gz")));
        assert!(is_archive_path(Path::new("FOO.TGZ")));
        assert!(is_archive_path(Path::new("Foo.Tar.Gz")));
        assert!(!is_archive_path(Path::new("foo.txt")));
        assert!(!is_archive_path(Path::new("foo")));
        // Currently unsupported but documented in the spec — must not
        // be flagged as archive so the fallback handles it.
        assert!(!is_archive_path(Path::new("foo.7z")));
    }

    #[test]
    fn read_zip_listing_returns_entries_with_uncompressed_size() {
        use std::io::Write;
        use zip::write::FileOptions;
        let dir = TempDir::new().expect("tempdir");
        let zip_path = dir.path().join("a.zip");
        let file = std::fs::File::create(&zip_path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        let options: FileOptions<()> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file("hello.txt", options).expect("start");
        writer.write_all(b"hello").expect("write");
        writer.start_file("notes/inner.md", options).expect("start");
        writer.write_all(b"# inner").expect("write");
        writer.finish().expect("finish zip");

        let listing = read_archive_listing(&zip_path).expect("zip listing");
        assert_eq!(listing.extra, 0);
        assert_eq!(
            listing.entries,
            vec![
                ArchiveEntry {
                    name: "hello.txt".into(),
                    size: Some(5),
                },
                ArchiveEntry {
                    name: "notes/inner.md".into(),
                    size: Some(7),
                },
            ],
        );
    }

    #[test]
    fn read_zip_listing_truncates_at_cap_and_reports_extra() {
        use zip::write::FileOptions;
        let dir = TempDir::new().expect("tempdir");
        let zip_path = dir.path().join("big.zip");
        let file = std::fs::File::create(&zip_path).expect("create");
        let mut writer = zip::ZipWriter::new(file);
        let options: FileOptions<()> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let total = ARCHIVE_ENTRIES_CAP + 5;
        for i in 0..total {
            writer
                .start_file(format!("f{i}.txt"), options)
                .expect("start");
        }
        writer.finish().expect("finish");

        let listing = read_archive_listing(&zip_path).expect("listing");
        assert_eq!(listing.entries.len(), ARCHIVE_ENTRIES_CAP);
        assert_eq!(listing.extra, 5);
    }

    #[test]
    fn read_tar_listing_returns_entries() {
        let dir = TempDir::new().expect("tempdir");
        let tar_path = dir.path().join("a.tar");
        let file = std::fs::File::create(&tar_path).expect("create");
        let mut builder = tar::Builder::new(file);
        let payload = b"hello tar";
        let mut header = tar::Header::new_gnu();
        header.set_path("inside.txt").expect("set path");
        header.set_size(payload.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append(&header, &payload[..])
            .expect("append entry");
        builder.finish().expect("finish");
        drop(builder);

        let listing = read_archive_listing(&tar_path).expect("listing");
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "inside.txt");
        assert_eq!(listing.entries[0].size, Some(payload.len() as u64));
    }

    #[test]
    fn read_tar_gz_listing_decompresses_then_lists() {
        let dir = TempDir::new().expect("tempdir");
        let tar_path = dir.path().join("a.tar.gz");
        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(&tar_path).expect("create"),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(gz);
        let payload = b"compressed";
        let mut header = tar::Header::new_gnu();
        header.set_path("doc.md").expect("set path");
        header.set_size(payload.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append(&header, &payload[..])
            .expect("append entry");
        let gz = builder.into_inner().expect("inner");
        gz.finish().expect("finish gz");

        let listing = read_archive_listing(&tar_path).expect("listing");
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "doc.md");
    }

    #[test]
    fn read_archive_listing_unknown_extension_is_none() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("a.txt");
        fs::write(&path, b"plain text").expect("write");
        assert!(read_archive_listing(&path).is_none());
    }

    #[test]
    fn is_image_path_matches_known_extensions_case_insensitive() {
        assert!(is_image_path(Path::new("foo.png")));
        assert!(is_image_path(Path::new("foo.JPG")));
        assert!(is_image_path(Path::new("foo.jpeg")));
        assert!(is_image_path(Path::new("foo.Gif")));
        assert!(is_image_path(Path::new("foo.webp")));
        assert!(is_image_path(Path::new("foo.bmp")));
        assert!(is_image_path(Path::new("foo.ico")));
        assert!(!is_image_path(Path::new("foo.txt")));
        assert!(!is_image_path(Path::new("foo")));
        assert!(!is_image_path(Path::new("foo.tiff")));
    }

    #[test]
    fn read_image_info_reads_dimensions_from_header() {
        // Minimal 1×1 PNG (the smallest valid image) — header parsing is
        // enough to report the dimensions without decoding pixel data.
        let png: [u8; 67] = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("pixel.png");
        fs::write(&path, png).expect("write png");
        let info = read_image_info(&path, "pixel.png".into(), png.len() as u64);
        assert_eq!(info.dimensions, Some((1, 1)));
        assert_eq!(info.mime, "image/png");
        assert_eq!(info.path, path);
    }

    #[test]
    fn read_image_info_unreadable_header_yields_none_dimensions() {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("broken.png");
        fs::write(&path, b"not a real png").expect("write");
        let info = read_image_info(&path, "broken.png".into(), 14);
        assert_eq!(info.dimensions, None);
        assert_eq!(info.mime, "image/png");
    }

    #[test]
    fn resolve_goto_query_handles_empty() {
        let cwd = Path::new("/tmp");
        assert_eq!(resolve_goto_query("", cwd), None);
        assert_eq!(resolve_goto_query("   ", cwd), None);
    }

    #[test]
    fn resolve_goto_query_absolute_passthrough() {
        let resolved = resolve_goto_query("/etc", Path::new("/tmp")).unwrap();
        assert_eq!(resolved, PathBuf::from("/etc"));
    }

    #[test]
    fn resolve_goto_query_relative_joins_with_cwd() {
        let resolved = resolve_goto_query("foo/bar", Path::new("/tmp/x")).unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/x/foo/bar"));
    }

    #[test]
    fn resolve_goto_query_expands_tilde() {
        if let Some(home) = std::env::var_os("HOME") {
            let resolved = resolve_goto_query("~/sub", Path::new("/anywhere")).unwrap();
            let expected = PathBuf::from(home).join("sub");
            assert_eq!(resolved, expected);
        }
    }

    #[test]
    fn extend_goto_completion_extends_unique_directory() {
        let dir = make_tree(&[("alpha", true), ("alpine", true), ("beta", true)]);
        let extended = extend_goto_completion("a", dir.path());
        assert_eq!(extended, "alp");
    }

    #[test]
    fn extend_goto_completion_appends_slash_on_single_dir_match() {
        let dir = make_tree(&[("alpha", true), ("beta", true)]);
        let extended = extend_goto_completion("al", dir.path());
        assert_eq!(extended, "alpha/");
    }

    #[test]
    fn extend_goto_completion_preserves_query_when_no_match() {
        let dir = make_tree(&[("alpha", true)]);
        let extended = extend_goto_completion("zzz", dir.path());
        assert_eq!(extended, "zzz");
    }

    #[test]
    fn longest_common_prefix_basic() {
        assert_eq!(
            longest_common_prefix(["abcd", "abce", "abcf"].iter().copied()),
            "abc"
        );
        assert_eq!(longest_common_prefix(["abc"].iter().copied()), "abc");
        assert_eq!(
            longest_common_prefix(["abc", "xyz"].iter().copied()),
            String::new()
        );
        assert_eq!(longest_common_prefix(std::iter::empty()), String::new());
    }

    #[test]
    fn split_dir_leaf_no_slash() {
        let (d, l) = split_dir_leaf("foo");
        assert_eq!(d, "");
        assert_eq!(l, "foo");
    }

    #[test]
    fn split_dir_leaf_with_slash() {
        let (d, l) = split_dir_leaf("a/b/c");
        assert_eq!(d, "a/b/");
        assert_eq!(l, "c");
    }

    #[test]
    fn natural_compare_orders_numbers_numerically() {
        assert_eq!(natural_compare("file2", "file10"), std::cmp::Ordering::Less);
        assert_eq!(natural_compare("file10", "file2"), std::cmp::Ordering::Greater);
        assert_eq!(natural_compare("file1", "file1"), std::cmp::Ordering::Equal);
        assert_eq!(natural_compare("abc", "abd"), std::cmp::Ordering::Less);
    }

    #[test]
    fn sort_entries_size_ascending_keeps_dirs_first() {
        let dir = make_tree(&[
            ("zzz", true),
            ("big.txt", false),
            ("small.txt", false),
        ]);
        std::fs::write(dir.path().join("big.txt"), vec![0u8; 1024]).expect("write big");
        std::fs::write(dir.path().join("small.txt"), vec![0u8; 8]).expect("write small");
        let entries = read_dir_sync(
            dir.path(),
            ReadDirOptions {
                sort: crate::prefs::SortMode::Size,
                ..ReadDirOptions::default()
            },
        );
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zzz", "small.txt", "big.txt"]);
    }

    #[test]
    fn sort_entries_reverse_flips_within_group() {
        let dir = make_tree(&[("a", true), ("b", true), ("x.txt", false), ("y.txt", false)]);
        let entries = read_dir_sync(
            dir.path(),
            ReadDirOptions {
                sort: crate::prefs::SortMode::Name,
                reverse: true,
                ..ReadDirOptions::default()
            },
        );
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "y.txt", "x.txt"]);
    }

    #[test]
    fn sort_entries_reverse_preserves_dirs_before_files() {
        let dir = make_tree(&[
            ("alpha-dir", true),
            ("beta-dir", true),
            ("alpha.txt", false),
            ("beta.txt", false),
        ]);
        let entries = read_dir_sync(
            dir.path(),
            ReadDirOptions {
                sort: crate::prefs::SortMode::Name,
                reverse: true,
                ..ReadDirOptions::default()
            },
        );
        let is_dir: Vec<bool> = entries.iter().map(|e| e.is_dir).collect();
        assert_eq!(is_dir, vec![true, true, false, false]);
    }

    #[test]
    fn sort_entries_extension_groups_by_suffix() {
        let dir = make_tree(&[
            ("note.md", false),
            ("main.rs", false),
            ("lib.rs", false),
            ("readme.txt", false),
        ]);
        let entries = read_dir_sync(
            dir.path(),
            ReadDirOptions {
                sort: crate::prefs::SortMode::Extension,
                ..ReadDirOptions::default()
            },
        );
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["note.md", "lib.rs", "main.rs", "readme.txt"]);
    }

    #[test]
    fn apply_chmod_input_octal_replaces_mode() {
        assert_eq!(apply_chmod_input("755", Some(0o644)), Ok(0o755));
        assert_eq!(apply_chmod_input("0755", Some(0o644)), Ok(0o755));
        assert_eq!(apply_chmod_input("0o755", Some(0o644)), Ok(0o755));
        // Set-uid bits round-trip.
        assert_eq!(apply_chmod_input("4755", Some(0o644)), Ok(0o4755));
    }

    #[test]
    fn apply_chmod_input_symbolic_combines_with_current() {
        // u+x flips owner-execute on a 644 file.
        assert_eq!(apply_chmod_input("u+x", Some(0o644)), Ok(0o744));
        // Default who = a; +x sets execute for everyone.
        assert_eq!(apply_chmod_input("+x", Some(0o644)), Ok(0o755));
        // Comma-separated clauses fold in order.
        assert_eq!(apply_chmod_input("u+x,go-w", Some(0o666)), Ok(0o744));
        // = wipes the targeted who-class first.
        assert_eq!(apply_chmod_input("a=r", Some(0o777)), Ok(0o444));
    }

    #[test]
    fn apply_chmod_input_rejects_bad_inputs() {
        assert!(apply_chmod_input("", Some(0o644)).is_err());
        // 9 is not a valid octal digit, and there's no operator either.
        assert!(apply_chmod_input("9", Some(0o644)).is_err());
        assert!(apply_chmod_input("u", Some(0o644)).is_err());
        assert!(apply_chmod_input("u+z", Some(0o644)).is_err());
        assert!(apply_chmod_input("u+x,", Some(0o644)).is_err());
    }
}
