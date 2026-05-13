use codon_mode::{CodonModeTracker, ObjectKind, PaneMode, Selection, SelectionSource};
use fs::{copy_recursive, CopyOptions, RenameOptions};
use git::status::FileStatus;
use gpui::{
    actions, prelude::*, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle,
    Focusable, KeyContext, ScrollStrategy, SharedString, Task, UniformListScrollHandle, WeakEntity,
    Window,
};
use std::cmp;
use std::collections::BTreeSet;
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
    ]
);

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
    /// First half of a two-key chord (e.g. `u` of `uv`). Cleared on the
    /// next keystroke whether or not the chord completed. Designed to
    /// host future bookmark chords (`m<letter>` / `'<letter>`) without
    /// further state.
    pub(crate) pending_chord: Option<char>,
    /// FM-local visual-line selection state. `None` means today's
    /// "single mark" behavior. `Some(anchor)` means `V` has been pressed
    /// at row `anchor` and j/k navigation now drives the marked range
    /// from there.
    ///
    /// This is intentionally NOT routed through `codon-mode::PaneMode`:
    /// the FM stays in `PaneMode::Normal` so the global mode tracker and
    /// keymap predicates keep working as they do today. Visual-range is
    /// a narrow, pane-local UI mode rather than a third top-level mode.
    pub(crate) visual_anchor: Option<usize>,
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
            pending_chord: None,
            visual_anchor: None,
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

    pub(crate) fn update_preview_sync(&mut self) {
        let Some(entry) = self.entries.get(self.selected_index) else {
            self.preview = Preview::Empty;
            return;
        };

        if entry.is_dir {
            let children = read_dir_sync(&entry.path, self.show_hidden);
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
                    | PendingInput::Rename { new_name: s, .. }
                    | PendingInput::BulkRename { pattern: s, .. } => {
                        s.pop();
                    }
                    PendingInput::Filter => {
                        self.filter_query.pop();
                        self.apply_filter();
                    }
                    PendingInput::ConfirmOverwrite { .. }
                    | PendingInput::ConfirmDeleteMarked { .. } => {
                        // Nothing to edit on the prompt; expected response is
                        // y/n or Esc.
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
                        match pending {
                            PendingInput::CreateFile(s)
                            | PendingInput::CreateDirectory(s)
                            | PendingInput::Rename { new_name: s, .. }
                            | PendingInput::BulkRename { pattern: s, .. } => {
                                s.push_str(ch);
                            }
                            PendingInput::Filter => {
                                self.filter_query.push_str(ch);
                                self.apply_filter();
                            }
                            PendingInput::ConfirmOverwrite { .. }
                            | PendingInput::ConfirmDeleteMarked { .. } => {
                                // Handled in the branch above.
                            }
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

        // Chord completion: only `uv` is wired today. The pending-chord
        // slot is consumed up-front so any non-matching second key
        // (e.g. `u` then `j`) falls through to the regular dispatch
        // with the chord already cleared.
        let pending_chord = self.pending_chord.take();
        if let Some('u') = pending_chord
            && !shift
            && !ctrl
            && key == "v"
        {
            self.clear_marks(cx);
            cx.stop_propagation();
            return;
        }

        // Visual-range housekeeping: any key outside the
        // extend / commit / mark-verb set drops the anchor before the
        // dispatch table sees the key. Marks themselves survive — they
        // become the input to the next y / d / p / D / R verb.
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
            "." => { self.toggle_hidden(&ToggleHidden, window, cx); true }
            // Fuzzy filter
            "/" => { self.start_filter(window, cx); true }
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


pub(crate) fn read_dir_sync(path: &Path, show_hidden: bool) -> Vec<DirEntry> {
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
        let entries = read_dir_sync(dir.path(), false);
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
        let entries = read_dir_sync(dir.path(), true);
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
        let entries = read_dir_sync(dir.path(), false);
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
        let entries = read_dir_sync(dir.path(), false);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["apple.txt", "Banana.txt", "Zebra.txt"]);
    }

    #[test]
    fn read_dir_sync_unreadable_path_returns_empty() {
        let entries = read_dir_sync(Path::new("/nonexistent/path/that/does/not/exist"), false);
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
}
