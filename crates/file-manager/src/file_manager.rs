use codon_mode::{CodonModeTracker, ObjectKind, PaneMode, Selection, SelectionSource};
use fs::{copy_recursive, CopyOptions, RenameOptions};
use git::status::FileStatus;
use gpui::{
    actions, point, prelude::*, Action, App, Bounds, ClipboardItem, Context, Entity, EventEmitter,
    FocusHandle, Focusable, KeyContext, Pixels, ScrollStrategy, SharedString, Size, Task,
    UniformListScrollHandle, WeakEntity, Window,
};
use language::{Buffer, LanguageRegistry};
use schemars::JsonSchema;
use serde::Deserialize;
use std::cmp;
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ui::{Icon, IconName};
use util::ResultExt;
use project::Project;
use workspace::{
    delete_unloaded_items, item::ItemEvent, register_serializable_item, Item, ItemId,
    SerializableItem, Workspace, WorkspaceId,
};

use crate::persistence::FileManagerDb;
use crate::prefs::LineMode;

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
        /// Show the choose-opener picker for the entry under the cursor.
        ChooseOpener,
        /// Sort entries alphabetically by name (case-insensitive).
        SortByName,
        /// Sort entries by size (smallest first).
        SortBySize,
        /// Sort entries by modification time (oldest first).
        SortByMtime,
        /// Sort entries by creation / birth time (oldest first).
        SortByBtime,
        /// Sort entries by file extension.
        SortByExtension,
        /// Sort entries using natural order so `file2` precedes `file10`.
        SortByNatural,
        /// Shuffle entries randomly within the dirs / files groups.
        SortByRandom,
        /// Flip the current sort direction (ascending ↔ descending).
        ToggleSortReverse,
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

/// Maximum number of `read_link` hops `resolve_with_depth_cap` will
/// follow before giving up. Matches the value most BSD / Linux kernels
/// use for `ELOOP` so the FM behaves consistently with shell tools.
const SYMLINK_DEPTH_CAP: usize = 16;

/// How long the `l`-as-chord arm waits for a followup before
/// committing `enter_directory`. Short enough that the bare `l`
/// muscle memory keeps feeling immediate; long enough that a
/// deliberate `l n` always lands in the chord branch even on a slow
/// keyboard. The global GPUI chord timeout (5 s) is intentionally
/// not reused here — that's tuned for `cmd-k` chord prefixes, not
/// for an action key that doubles as a chord starter.
const L_CHORD_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(220);

/// Time the preview load waits after a selection change before doing
/// any filesystem work. Short enough that a single deliberate `j` feels
/// instant, long enough that holding `j` down (~16 ms/keystroke under
/// auto-repeat) coalesces every keystroke except the last into a single
/// preview read.
const PREVIEW_DEBOUNCE_MS: u64 = 40;

/// How many recently-visited directories `DirCache` retains. Tuned for
/// the back/forward + parent-and-back navigation cycle: 8 covers a few
/// rounds of "into a subdir, back out, into a sibling" without evicting
/// the dirs the user is bouncing between.
const DIR_CACHE_CAP: usize = 8;

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
    /// Number of immediate children when `is_dir`; `None` for files or
    /// when the directory could not be opened (permission denied, etc).
    pub(crate) child_count: Option<usize>,
    /// Precomputed render labels — see `EntryLabels::build`. Filled
    /// at construction time in `read_dir_sync` (or when `child_count`
    /// gets backfilled, via `recompute_labels`) so the hot-path render
    /// closures only clone an `Arc` instead of allocating a new
    /// `String` per visible row per frame.
    pub(crate) labels: EntryLabels,
}

/// Cached render text for a `DirEntry`. `name` is the entry name as a
/// `SharedString` (cheap `Arc` clone on render); `meta` holds one
/// precomputed `SharedString` per `LineMode` variant, indexed by
/// `LineMode::idx`. `None` slots mean "this mode renders no meta cell
/// for this entry" (e.g. directories with no `child_count` filled yet,
/// `LineMode::None` always).
#[derive(Clone, Default)]
pub(crate) struct EntryLabels {
    pub(crate) name: gpui::SharedString,
    pub(crate) meta: [Option<gpui::SharedString>; LineMode::COUNT],
}

#[derive(Clone)]
pub(crate) enum Preview {
    Directory(Vec<DirEntry>),
    Text(TextPreview),
    Archive(ArchiveListing),
    Image(ImageInfo),
    Binary(BinaryInfo),
    Empty,
}

/// Source bytes for a text preview. The heavy `editor::Editor` entity
/// that actually renders the content is built lazily by `view.rs` and
/// cached on the `FileManager` keyed by `path`, so rapid `j`/`k`
/// scrolling reuses the same editor instead of allocating per
/// keystroke.
#[derive(Clone)]
pub(crate) struct TextPreview {
    pub(crate) path: PathBuf,
    pub(crate) content: String,
}

/// Cached read-only `editor::Editor` for the currently previewed text
/// file. Held outside the `Preview` enum so swapping preview variants
/// (text → directory → text) doesn't churn the entity unless the
/// previewed path actually changes.
pub(crate) struct PreviewEditorCache {
    pub(crate) path: PathBuf,
    pub(crate) editor: Entity<editor::Editor>,
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
    /// Bumped on every directory load so the background `child_count`
    /// fill task can detect "the listing I was spawned for is gone" and
    /// drop its result without touching `entries`.
    pub(crate) listing_gen: u64,
    /// Bumped on every preview request so a debounced/in-flight load
    /// that's been superseded can drop its result silently. See
    /// `request_preview_update`.
    pub(crate) preview_gen: u64,
    /// Path the most recent `request_preview_update` is loading (or has
    /// already loaded). A repeat request for the same path is a no-op,
    /// which coalesces rapid `j`/`k` traffic on the same selection.
    pub(crate) preview_target: Option<PathBuf>,
    /// LRU of recently-visited directory listings — see `DirCache`.
    /// Shared with background listing tasks behind a `Mutex`.
    pub(crate) dir_cache: Arc<std::sync::Mutex<DirCache>>,
    pub(crate) clipboard: FmClipboard,
    pub(crate) back_stack: VecDeque<PathBuf>,
    pub(crate) forward_stack: VecDeque<PathBuf>,
    pub(crate) pending_chord: Option<char>,
    /// Bumped every time `pending_chord` is armed. Lets a delayed
    /// fallback task (notably the `l`-as-chord-or-enter timer) tell
    /// "the chord I was scheduled for is still the active one" from
    /// "a fresh chord landed in the meantime" and skip the fallback
    /// in the latter case.
    pub(crate) pending_chord_gen: u64,
    pub(crate) visual_anchor: Option<usize>,
    pub(crate) sort: crate::prefs::SortMode,
    pub(crate) reverse: bool,
    pub(crate) line_mode: crate::prefs::LineMode,
    pub(crate) show_gitignored: bool,
    pub(crate) preview_fraction: f32,
    pub(crate) last_find_pattern: Option<String>,
    pub(crate) shell_running: Option<ShellRunState>,
    /// Workspace-wide language registry. Resolved once at construction
    /// time so the preview builder doesn't have to climb back up to the
    /// workspace for every selection change. `None` only in test setups
    /// where the FM is spun up without a workspace entity.
    pub(crate) language_registry: Option<Arc<LanguageRegistry>>,
    /// Read-only editor used to render the currently previewed text
    /// file. Reused across selection changes within the same file;
    /// rebuilt when the selected path changes.
    pub(crate) preview_editor: Option<PreviewEditorCache>,
    /// True while Cmd is the only modifier currently held in this
    /// window. Drives the bottom-bar Cmd-shortcut overlay; updated
    /// via the `on_modifiers_changed` handler.
    pub(crate) cmd_only_held: bool,
    /// Last measured pixel width of the parent (left) column,
    /// captured from `on_children_prepainted`. Drives the
    /// responsive hide-meta-when-narrow behavior so filenames in
    /// the dimmed context column don't get squeezed by the
    /// fixed-width meta gutter. 0.0 until the first paint.
    pub(crate) parent_col_width: f32,
    /// Last measured pixel width of the full columns row (the span
    /// from the leftmost child's left edge to the rightmost
    /// child's right edge). Drives the responsive column hiding:
    /// drop the parent column when there isn't enough room for
    /// three, then drop the preview column too when even two
    /// would be cramped. 0.0 until the first paint.
    pub(crate) fm_total_width: f32,
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
    ConfirmSkipTrashDelete { targets: Vec<PathBuf> },
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
    ContentSearchQuery(String),
    ShellBlocking { input: String },
    ShellAsync { input: String },
}

pub(crate) struct ShellRunState {
    pub(crate) command: String,
    pub(crate) terminal: gpui::WeakEntity<terminal_view::TerminalView>,
    pub(crate) escape_count: u8,
    pub(crate) _watcher: Task<()>,
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
    /// Absolute path of the directory the file manager is currently
    /// browsing. Used by codon-session's contextual split actions
    /// (`SplitTerminal*` / `SplitFileManager*`) to seed the newly
    /// opened pane with the caller's location.
    pub fn current_directory(&self) -> &Path {
        &self.current_dir
    }

    pub fn new(
        initial_dir: PathBuf,
        workspace: WeakEntity<Workspace>,
        fs: Arc<dyn fs::Fs>,
        language_registry: Option<Arc<LanguageRegistry>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        // Mode-tracker updates flow through codon-mode's `PaneModeBridge`
        // dispatcher — see [`PaneModeBridge`] impl below. Local focus
        // handling stays here only for FM-private housekeeping (git
        // status refresh + re-render).
        cx.on_focus(&focus_handle, window, |this: &mut Self, _window, cx| {
            this.populate_git_status(cx);
            cx.notify();
        })
        .detach();

        // Re-render the footer mode badge as the tracker flips between
        // Normal / Insert / Command. We don't hold the Subscription —
        // the panel lives for the lifetime of the app, so detaching is
        // safe and avoids growing the struct.
        cx.observe_global::<CodonModeTracker>(|_, cx| cx.notify())
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
            listing_gen: 0,
            preview_gen: 0,
            preview_target: None,
            dir_cache: Arc::new(std::sync::Mutex::new(DirCache::default())),
            clipboard: FmClipboard::Empty,
            back_stack: VecDeque::new(),
            forward_stack: VecDeque::new(),
            pending_chord: None,
            pending_chord_gen: 0,
            visual_anchor: None,
            sort: prefs.sort,
            reverse: prefs.reverse,
            line_mode: prefs.line_mode,
            show_gitignored: prefs.show_gitignored,
            preview_fraction: crate::prefs::clamp_fraction(prefs.preview_fraction),
            last_find_pattern: None,
            shell_running: None,
            language_registry,
            preview_editor: None,
            cmd_only_held: false,
            parent_col_width: 0.0,
            fm_total_width: 0.0,
        };
        // Kick off the initial listing via the same async path that
        // navigation uses — the FM renders briefly empty (one frame at
        // worst) before the read_dir lands, which is the right trade
        // for keeping one code path.
        this.reload_entries(window, cx);
        // Register the jump provider with codon-jump's global registry
        // so `cmd-k j` paints a chip on every visible file-manager row.
        // The provider holds a WeakEntity, so the registry's
        // `is_alive`-based pruning collects it once this FileManager
        // drops — no explicit teardown needed.
        crate::jump_provider::register(cx);
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

    /// Clear listing-derived state ahead of a reload. Marks/filter belong
    /// to the *old* listing — keeping them visible against the upcoming
    /// (different) entries would be wrong. They're cleared synchronously
    /// at spawn time so the screen never shows mismatched mark indices,
    /// even though the new entries themselves arrive asynchronously from
    /// `reload_entries_with`.
    fn prepare_reload(&mut self) {
        self.marked.clear();
        // A fresh directory listing invalidates any active filter — the
        // user navigated, so the original set is gone.
        self.filter_query.clear();
        self.entries_unfiltered = None;
        // Bump so any in-flight child-count fill / preview / listing task
        // from a previous reload sees the gen mismatch and drops its
        // result without mutating `self`.
        self.listing_gen = self.listing_gen.wrapping_add(1);
        // Invalidate the preview target so a navigation back to the same
        // selected path still triggers a fresh preview load.
        self.preview_target = None;
    }

    fn reload_entries(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_entries_with(None, cx);
    }

    /// Read the current directory + its parent on the background
    /// executor, then install the result on the foreground. The previous
    /// `entries` keep rendering until the new ones arrive (no flicker).
    ///
    /// `select_after`, if `Some`, is the entry name to focus once the
    /// new listing lands — used by `parent_directory` (jump back to the
    /// child folder we just came out of) and by `reveal_path`. With
    /// `None`, the selection is clamped to the new list length.
    fn reload_entries_with(
        &mut self,
        select_after: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.prepare_reload();
        let listing_gen = self.listing_gen;
        let current_dir = self.current_dir.clone();
        let opts = self.read_dir_options();
        let want_index = self.selected_index;
        let cache = Arc::clone(&self.dir_cache);

        cx.spawn(async move |this, cx| {
            let parent_path = current_dir.parent().map(|p| p.to_path_buf());
            let dir_for_read = current_dir.clone();
            let (entries, parent_entries) = cx
                .background_executor()
                .spawn(async move {
                    let entries = read_dir_cached(&cache, &dir_for_read, opts);
                    let parent_entries = parent_path
                        .as_deref()
                        .map(|p| read_dir_cached(&cache, p, opts))
                        .unwrap_or_default();
                    (entries, parent_entries)
                })
                .await;

            this.update(cx, |this, cx| {
                if this.listing_gen != listing_gen {
                    return;
                }
                this.entries = entries;
                this.parent_entries = parent_entries;
                if let Some(name) = select_after.as_deref() {
                    this.selected_index = this
                        .entries
                        .iter()
                        .position(|e| e.name == name)
                        .unwrap_or(0);
                } else {
                    this.selected_index =
                        cmp::min(want_index, this.entries.len().saturating_sub(1));
                }
                this.populate_git_status(cx);
                this.ensure_visible();
                this.spawn_child_count_fill(cx);
                this.request_preview_update(cx);
                cx.emit(FileManagerEvent::PathChanged);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Populate `child_count` on the current + parent column entries from
    /// a background task. `read_dir_sync` deliberately leaves them `None`
    /// — counting children would otherwise cost one extra `read_dir` per
    /// subdirectory on the foreground thread.
    ///
    /// Stale results (user navigated away before the count returned) are
    /// dropped via the `listing_gen` check.
    fn spawn_child_count_fill(&self, cx: &mut Context<Self>) {
        let listing_gen = self.listing_gen;
        let targets: Vec<PathBuf> = self
            .entries
            .iter()
            .chain(self.parent_entries.iter())
            .filter(|e| e.is_dir && e.child_count.is_none())
            .map(|e| e.path.clone())
            .collect();
        if targets.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let counts: Vec<(PathBuf, usize)> = cx
                .background_executor()
                .spawn(async move {
                    targets
                        .into_iter()
                        .filter_map(|p| {
                            let n = std::fs::read_dir(&p).ok()?.count();
                            Some((p, n))
                        })
                        .collect()
                })
                .await;
            this.update(cx, |this, cx| {
                if this.listing_gen != listing_gen {
                    return;
                }
                // Path-based update so an active filter (which mutates
                // `entries` in place but doesn't bump the gen) is handled
                // correctly: missing paths just no-op.
                let mut map: std::collections::HashMap<PathBuf, usize> =
                    counts.into_iter().collect();
                for entry in this
                    .entries
                    .iter_mut()
                    .chain(this.parent_entries.iter_mut())
                    .chain(this.entries_unfiltered.iter_mut().flat_map(|v| v.iter_mut()))
                {
                    if entry.child_count.is_some() {
                        continue;
                    }
                    if let Some(n) = map.remove(&entry.path) {
                        entry.child_count = Some(n);
                        // Refresh the cached `LineMode::Size` label so the
                        // newly-known count actually renders. Without this,
                        // the `Size`-mode meta cell would keep showing
                        // blank (the slot was `None` when labels were first
                        // built at `child_count: None`).
                        entry.labels = build_entry_labels(entry);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn select_entry_by_name(&mut self, name: &str, cx: &mut Context<Self>) {
        if let Some(idx) = self.entries.iter().position(|e| e.name == name) {
            self.selected_index = idx;
            self.ensure_visible();
            self.request_preview_update(cx);
            cx.notify();
        }
    }

    /// Move the cursor to `row`, clamped to the entry list. Used by
    /// the jump-hint overlay (`codon-jump`) when a label is dispatched
    /// against a file-manager row.
    pub fn set_cursor_index(&mut self, row: usize, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        let clamped = cmp::min(row, self.entries.len() - 1);
        if clamped == self.selected_index {
            return;
        }
        self.selected_index = clamped;
        self.scroll_handle
            .scroll_to_item(self.selected_index, ScrollStrategy::Nearest);
        self.request_preview_update(cx);
        cx.notify();
        self.notify_workspace_scrolled(cx);
    }

    /// Index of the topmost row currently inside the viewport. Reads
    /// the `UniformListScrollHandle`'s base scroll state directly;
    /// before the first paint (when the list hasn't laid out yet) the
    /// `top_item` call falls back to 0, which is what the jump
    /// provider wants for a freshly-opened panel.
    pub fn first_visible_row(&self) -> usize {
        let state = self.scroll_handle.0.borrow();
        // If a `scroll_to_item` is deferred but not yet applied, prefer
        // the target index so that hints assigned right after a `Ctrl-d`
        // line up with where the user is about to see the list — the
        // base handle's `top_item` still points at the pre-scroll row.
        if let Some(deferred) = state.deferred_scroll_to_item.as_ref() {
            return deferred.item_index.min(self.entries.len().saturating_sub(1));
        }
        state.base_handle.top_item()
    }

    /// Number of visible rows in the file-manager viewport, derived
    /// from the captured item size. Returns `0` before the first
    /// layout pass — callers (notably the jump provider) treat zero
    /// rows as "no candidates".
    pub fn visible_row_count(&self) -> usize {
        let state = self.scroll_handle.0.borrow();
        let Some(item_size) = state.last_item_size else {
            return 0;
        };
        let row_height = item_size.item.height;
        if row_height <= Pixels::from(0.0) {
            return 0;
        }
        let viewport_height = state.base_handle.bounds().size.height;
        let raw = (f32::from(viewport_height) / f32::from(row_height)).ceil() as i64;
        if raw <= 0 {
            return 0;
        }
        let raw = raw as usize;
        // Cap at the list length so the last partially-visible row is
        // still represented but we never produce candidates pointing
        // past the entry tail.
        raw.min(self.entries.len().saturating_sub(self.first_visible_row()))
    }

    /// Screen-space bounds of `row` in window-absolute pixel space, or
    /// `None` when the list hasn't laid out yet (`last_item_size` not
    /// captured) or `row` is outside the visible window. Computes the
    /// position analytically from the captured row height + the list's
    /// own bounds — uniform-list rows are fixed-height, so we don't
    /// need to consult per-row bounds (which the handle doesn't
    /// expose).
    pub fn row_screen_bounds(&self, row: usize, _cx: &App) -> Option<Bounds<Pixels>> {
        let state = self.scroll_handle.0.borrow();
        let item_size = state.last_item_size?;
        let row_height = item_size.item.height;
        if row_height <= Pixels::from(0.0) {
            return None;
        }
        let list_bounds = state.base_handle.bounds();
        if list_bounds.size.height <= Pixels::from(0.0) {
            return None;
        }
        let first = if let Some(deferred) = state.deferred_scroll_to_item.as_ref() {
            deferred.item_index.min(self.entries.len().saturating_sub(1))
        } else {
            state.base_handle.top_item()
        };
        if row < first {
            return None;
        }
        let row_offset_from_top = (row - first) as f32 * f32::from(row_height);
        let row_top = list_bounds.origin.y + Pixels::from(row_offset_from_top);
        if row_top >= list_bounds.origin.y + list_bounds.size.height {
            return None;
        }
        let width = list_bounds.size.width;
        Some(Bounds {
            origin: point(list_bounds.origin.x, row_top),
            size: Size {
                width,
                height: row_height,
            },
        })
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
            // Thread the focus-target through the async reload so it's
            // applied inside the apply closure when entries are
            // available — calling `select_entry_by_name` here would
            // run before the new listing has loaded.
            self.reload_entries_with(select_name, cx);
        } else if let Some(name) = select_name {
            // Same dir — entries are already current, select inline.
            self.select_entry_by_name(&name, cx);
        }
        let _ = window;
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

    /// Return (creating if necessary) the read-only editor entity used
    /// to render the currently previewed text file. Cached on
    /// `self.preview_editor` keyed by path so rapid scrolling reuses
    /// the same editor when the selected file hasn't changed.
    pub(crate) fn preview_editor_for(
        &mut self,
        text: &TextPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<editor::Editor> {
        if let Some(cache) = self.preview_editor.as_ref()
            && cache.path == text.path
        {
            return cache.editor.clone();
        }

        let content = text.content.clone();
        let path = text.path.clone();
        let registry = self.language_registry.clone();

        let buffer = cx.new(|cx| {
            let buffer = Buffer::local(content, cx);
            if let Some(registry) = registry.clone() {
                buffer.set_language_registry(registry);
            }
            buffer
        });

        if let Some(registry) = registry {
            let buffer_handle = buffer.clone();
            cx.spawn(async move |_, cx| {
                // `path` is moved into the closure so the borrow taken by
                // `load_language_for_file_path` lives no longer than the
                // future itself (its return type carries the borrow's
                // lifetime). Constructing the future outside the spawn
                // would tie the borrow to the caller's stack frame, which
                // doesn't outlive the spawned task.
                let language = registry
                    .load_language_for_file_path(&path)
                    .await
                    .log_err()?;
                buffer_handle.update(cx, |buffer, cx| {
                    buffer.set_language(Some(language), cx);
                });
                Some(())
            })
            .detach();
        }

        let editor = cx.new(|cx| {
            let multi_buffer = cx.new(|cx| multi_buffer::MultiBuffer::singleton(buffer, cx));
            let mut editor = editor::Editor::new(
                editor::EditorMode::Full {
                    scale_ui_elements_with_buffer_font_size: false,
                    show_active_line_background: false,
                    sizing_behavior: editor::SizingBehavior::SizeByContent,
                },
                multi_buffer,
                None,
                window,
                cx,
            );
            editor.set_read_only(true);
            editor.set_show_gutter(false, cx);
            editor.set_show_line_numbers(false, cx);
            editor.set_show_scrollbars(false, cx);
            editor
        });

        self.preview_editor = Some(PreviewEditorCache {
            path: text.path.clone(),
            editor: editor.clone(),
        });

        editor
    }

    /// Request the preview pane to update for the currently selected
    /// entry. The actual filesystem reads run on the background executor
    /// after a short debounce window, so rapid `j`/`k` scrolling no
    /// longer queues N synchronous reads on the foreground thread.
    ///
    /// Coalescing rules:
    /// - If the new target path matches `preview_target`, no-op.
    /// - Otherwise `preview_gen` is bumped, the previous in-flight task
    ///   is logically cancelled (it sees the mismatched gen and drops
    ///   its result without touching `self`), and a new task is spawned.
    ///
    /// The previous preview keeps rendering until the new one lands —
    /// blanking the pane during scroll would feel like a flicker.
    pub(crate) fn request_preview_update(&mut self, cx: &mut Context<Self>) {
        let target_path = self
            .entries
            .get(self.selected_index)
            .map(|e| e.path.clone());

        if target_path == self.preview_target {
            return;
        }

        self.preview_gen = self.preview_gen.wrapping_add(1);
        self.preview_target = target_path.clone();
        let preview_gen = self.preview_gen;

        let Some(entry) = self.entries.get(self.selected_index).cloned() else {
            self.preview = Preview::Empty;
            self.preview_editor = None;
            cx.notify();
            return;
        };
        let opts = self.read_dir_options();
        let cache = Arc::clone(&self.dir_cache);

        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(PREVIEW_DEBOUNCE_MS))
                .await;
            // Quick gen-check before doing the actual read — a fresher
            // request may have arrived during the debounce window.
            let still_current = this
                .update(cx, |this, _| this.preview_gen == preview_gen)
                .unwrap_or(false);
            if !still_current {
                return;
            }

            let new_preview = cx
                .background_executor()
                .spawn(async move { compute_preview(&entry, opts, &cache) })
                .await;

            this.update(cx, |this, cx| {
                if this.preview_gen != preview_gen {
                    return;
                }
                this.preview = new_preview;
                // `preview_editor` is keyed on the previewed path; the
                // view layer rebuilds it on next render if the path
                // changed, so we don't need to invalidate it here.
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn ensure_visible(&self) {
        // Only scroll when the selected item would be off-screen.
        // Non-strict: does nothing if already visible.
        self.scroll_handle
            .scroll_to_item(self.selected_index, ScrollStrategy::Center);
    }

    fn notify_workspace_scrolled(&self, cx: &mut Context<Self>) {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.notify_scrolled(cx));
        }
    }

    fn navigate_down(&mut self, _: &NavigateDown, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = cmp::min(self.selected_index + 1, self.entries.len() - 1);
            self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Bottom);
            self.refresh_visual_marks();
            self.request_preview_update(cx);
            cx.notify();
            self.notify_workspace_scrolled(cx);
        }
    }

    fn navigate_up(&mut self, _: &NavigateUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = self.selected_index.saturating_sub(1);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.refresh_visual_marks();
        self.request_preview_update(cx);
        cx.notify();
        self.notify_workspace_scrolled(cx);
    }

    fn half_page_down(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            let half = self.visible_lines / 2;
            self.selected_index = cmp::min(self.selected_index + half, self.entries.len() - 1);
            self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Bottom);
            self.request_preview_update(cx);
            cx.notify();
            self.notify_workspace_scrolled(cx);
        }
    }

    fn half_page_up(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let half = self.visible_lines / 2;
        self.selected_index = self.selected_index.saturating_sub(half);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.request_preview_update(cx);
        cx.notify();
        self.notify_workspace_scrolled(cx);
    }

    fn page_down(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = cmp::min(
                self.selected_index + self.visible_lines,
                self.entries.len() - 1,
            );
            self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Bottom);
            self.request_preview_update(cx);
            cx.notify();
            self.notify_workspace_scrolled(cx);
        }
    }

    fn page_up(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = self.selected_index.saturating_sub(self.visible_lines);
        self.scroll_handle.scroll_to_item(self.selected_index, ScrollStrategy::Top);
        self.request_preview_update(cx);
        cx.notify();
        self.notify_workspace_scrolled(cx);
    }

    /// Enter the focused entry — for directories that means descending
    /// into them, for files that means handing off to the workspace's
    /// open path. Symlinks are followed implicitly: `entry.is_dir` is
    /// populated from `std::fs::metadata` which dereferences links, so
    /// a symlink pointing at a directory is treated the same as the
    /// directory itself.
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
            return;
        }

        self.open_focused_file(entry.path, window, cx);
    }

    /// File-branch routing for `enter_directory`. Consults the
    /// `OpenerStore` first:
    ///
    /// - unique match → spawn it through the existing shell-exec
    ///   dispatch, fully respecting marked-set semantics (so Enter on
    ///   any marked entry runs the opener for the whole marked set
    ///   when the user has marks live);
    /// - multiple matches → surface the `O` picker so the user picks
    ///   explicitly instead of guessing the first match;
    /// - zero matches → fall through to today's
    ///   `workspace.open_abs_path` path.
    ///
    /// The fallthrough preserves Zed's project-item registry behavior
    /// for files codon already knows how to handle (text, images, …),
    /// so users with no opener config keep today's UX exactly.
    fn open_focused_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let matches = cx
            .try_global::<crate::openers::OpenerStore>()
            .map(|s| s.matches_for(&path))
            .unwrap_or_default();

        match matches.len() {
            0 => self.open_paths_default(vec![path], window, cx),
            1 => {
                // SAFETY: matches.len() == 1 in this arm, so `next()` is Some.
                let Some(opener) = matches.into_iter().next() else {
                    return;
                };
                let targets = self.opener_targets();
                self.run_opener_choice(
                    crate::openers::OpenerChoice::Opener(opener),
                    targets,
                    window,
                    cx,
                );
            }
            _ => self.choose_opener(window, cx),
        }
    }

    /// `F` (shift-f): resolve the focused entry's symlink chain and
    /// reveal the final target in its parent directory. No-op when the
    /// focused entry is not a symlink. Loops are bounded by
    /// `resolve_with_depth_cap` so a pathological `a -> b -> a` does
    /// not hang the FM.
    fn follow_symlink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(self.selected_index).cloned() else {
            return;
        };
        if !entry.is_symlink {
            return;
        }
        let target = match resolve_with_depth_cap(&entry.path, SYMLINK_DEPTH_CAP) {
            Ok(t) => t,
            Err(e) => {
                self.surface_error(format!("Couldn't resolve symlink: {e}"), cx);
                return;
            }
        };
        window.dispatch_action(Box::new(Reveal(target)), cx);
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
            // Jumping back to the directory we just came out of has to
            // happen inside the apply closure — the entries don't exist
            // yet at this point.
            self.reload_entries_with(old_dir_name, cx);
            let _ = window;
        }
    }

    fn go_to_top(&mut self, _: &GoToTop, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_index = 0;
        self.ensure_visible();
        self.request_preview_update(cx);
        cx.notify();
    }

    fn go_to_bottom(&mut self, _: &GoToBottom, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
            self.ensure_visible();
            self.request_preview_update(cx);
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
        cx.notify();
    }

    /// Arm the `l`-as-chord state and schedule a fallback that fires
    /// `enter_directory` if no chord followup (a `n` for symlink, or
    /// any other key that would also commit the bare `l`) lands
    /// within `L_CHORD_TIMEOUT`. The generation counter lets the
    /// timer detect whether the chord it was scheduled for is still
    /// the active one.
    fn arm_l_chord(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_chord = Some('l');
        self.pending_chord_gen = self.pending_chord_gen.wrapping_add(1);
        let captured_gen = self.pending_chord_gen;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(L_CHORD_TIMEOUT).await;
            this.update_in(cx, |this, window, cx| {
                if this.pending_chord == Some('l') && this.pending_chord_gen == captured_gen {
                    this.pending_chord = None;
                    this.enter_directory(&EnterDirectory, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// `ln` chord verb: for each target (the marked set, or just the
    /// cursor entry when no marks exist) create a symlink in
    /// `current_dir` whose name is the target's basename. Conflicts
    /// fall back to the same numbered-suffix scheme `next_available_path`
    /// uses for paste, so `foo` becomes `foo (2)` and so on. The
    /// filesystem call is routed through `Fs::create_symlink` so a
    /// fake `Fs` in tests can intercept it the same way it does for
    /// rename / copy.
    fn make_symlinks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sources = self.current_targets();
        if sources.is_empty() {
            return;
        }
        let destination_dir = self.current_dir.clone();
        let mut used: Vec<PathBuf> = Vec::with_capacity(sources.len());
        let mut plan: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(sources.len());
        for source in sources {
            let Some(file_name) = source.file_name() else {
                continue;
            };
            let initial = destination_dir.join(file_name);
            let destination = if initial.exists() || used.iter().any(|p| p == &initial) {
                next_available_path(&destination_dir, file_name, &used)
            } else {
                initial
            };
            used.push(destination.clone());
            plan.push((source, destination));
        }
        if plan.is_empty() {
            return;
        }

        let fs = self.fs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
            for (source, destination) in &plan {
                if let Err(e) = fs.create_symlink(destination, source.clone()).await {
                    failures.push((source.clone(), e));
                }
            }
            this.update_in(cx, |this, window, cx| {
                for (path, e) in &failures {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    this.surface_error(format!("Couldn't link {name}: {e}"), cx);
                }
                this.marked.clear();
                this.reload_entries(window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// `Ln` chord verb: like `make_symlinks` but routes through
    /// `Fs::create_hardlink`. Hardlinks fail when source and target
    /// live on different filesystems with a cryptic `EXDEV` errno —
    /// we pre-check `metadata.dev()` on unix so we can surface a
    /// readable toast before attempting. On non-unix we skip the
    /// pre-check and let the trait method bubble up whatever error
    /// the OS returns.
    fn make_hardlinks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sources = self.current_targets();
        if sources.is_empty() {
            return;
        }
        let destination_dir = self.current_dir.clone();

        #[cfg(unix)]
        let destination_dev: Option<u64> = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&destination_dir)
                .ok()
                .map(|meta| meta.dev())
        };

        let mut used: Vec<PathBuf> = Vec::with_capacity(sources.len());
        let mut plan: Vec<(PathBuf, PathBuf)> = Vec::with_capacity(sources.len());
        let mut cross_fs: Vec<PathBuf> = Vec::new();
        for source in sources {
            let Some(file_name) = source.file_name() else {
                continue;
            };

            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if let (Some(dst_dev), Ok(src_meta)) =
                    (destination_dev, std::fs::metadata(&source))
                    && src_meta.dev() != dst_dev
                {
                    cross_fs.push(source);
                    continue;
                }
            }

            let initial = destination_dir.join(file_name);
            let destination = if initial.exists() || used.iter().any(|p| p == &initial) {
                next_available_path(&destination_dir, file_name, &used)
            } else {
                initial
            };
            used.push(destination.clone());
            plan.push((source, destination));
        }

        for source in &cross_fs {
            let name = source
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| source.display().to_string());
            self.surface_error(
                format!("Couldn't hardlink {name}: cannot hardlink across filesystems"),
                cx,
            );
        }

        if plan.is_empty() {
            return;
        }

        let fs = self.fs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
            for (source, destination) in &plan {
                if let Err(e) = fs.create_hardlink(destination, source.clone()).await {
                    failures.push((source.clone(), e));
                }
            }
            this.update_in(cx, |this, window, cx| {
                for (path, e) in &failures {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    this.surface_error(format!("Couldn't hardlink {name}: {e}"), cx);
                }
                this.marked.clear();
                this.reload_entries(window, cx);
            })
            .ok();
        })
        .detach();
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

    /// Action-bus entry point for `file_manager::ChooseOpener` — the
    /// keymap-bound twin of the `O` keypress arm in
    /// `handle_normal_keystroke`. Lets a user rebind the verb from
    /// `~/.config/codon/codon.toml` without losing the default `O` key.
    pub(crate) fn handle_choose_opener(
        &mut self,
        _: &ChooseOpener,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_opener(window, cx);
        cx.stop_propagation();
    }

    pub(crate) fn handle_cancel(
        &mut self,
        _: &menu::Cancel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Esc always wins: dismiss any active input prompt, drop back to
        // Normal mode, and clear a committed filter if one is showing.
        // Mirrors `find`'s Esc — which the user calls out as the desired
        // pattern — by being unconditional and idempotent.
        let pending = self.pending_input.take();
        let was_find_prompt = matches!(
            pending,
            Some(PendingInput::FindForward { .. } | PendingInput::FindBackward { .. })
        );
        let find_origin = match pending {
            Some(PendingInput::FindForward { origin_index, .. })
            | Some(PendingInput::FindBackward { origin_index, .. }) => Some(origin_index),
            _ => None,
        };
        let had_pending = pending.is_some();
        let had_filter = !self.filter_query.is_empty() || self.entries_unfiltered.is_some();
        if !had_pending && !had_filter && self.pending_chord.is_none() && self.visual_anchor.is_none() {
            return;
        }
        self.pending_chord = None;
        if self.visual_anchor.is_some() && !was_find_prompt {
            // Esc out of visual-line is a commit (mirrors helix); keep
            // marks but drop the anchor so j/k stop extending.
            self.visual_anchor = None;
        }
        self.mode = PaneMode::Normal;
        if had_filter {
            self.clear_filter(cx);
        }
        if let Some(origin) = find_origin {
            self.selected_index = cmp::min(
                origin,
                self.entries.len().saturating_sub(1),
            );
            self.ensure_visible();
            self.request_preview_update(cx);
        }
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
    fn apply_find_preview(
        &mut self,
        query: &str,
        origin_index: usize,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        if query.is_empty() {
            self.selected_index = cmp::min(origin_index, self.entries.len().saturating_sub(1));
            self.ensure_visible();
            self.request_preview_update(cx);
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
            self.request_preview_update(cx);
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
            self.request_preview_update(cx);
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
            self.request_preview_update(cx);
            cx.notify();
        }
    }

    /// `s`: open the name-search picker rooted at `current_dir`. The
    /// picker uses `fd` if installed and falls back to a synchronous
    /// `walkdir` capped at 5000 entries. Enter on a result reveals it
    /// via the `codon_fm::Reveal` action.
    fn open_search_by_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let root = self.current_dir.clone();
        let weak = self.workspace.clone();
        workspace.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, move |window, cx| {
                crate::search::NameSearchModal::new(root, weak, window, cx)
            });
        });
    }

    fn open_trash_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let weak = cx.weak_entity();
        workspace.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, move |window, cx| {
                crate::trash::TrashModal::new(weak, window, cx)
            });
        });
    }

    fn open_task_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let weak_workspace = self.workspace.clone();
        workspace.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, move |window, cx| {
                crate::task_history_modal::TaskHistoryModal::new(weak_workspace, window, cx)
            });
        });
    }

    fn open_search_by_content(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !crate::search::binary_available("rg") {
            self.surface_error("Install ripgrep for content search", cx);
            return;
        }
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::ContentSearchQuery(String::new()));
        cx.notify();
    }

    /// `Z` (shift-z): open the zoxide picker. Plain `z` is the chord
    /// starter for `zg` (toggle gitignore), so phase-7 keeps zoxide
    /// on shift-Z to avoid clobbering the chord dispatcher. Missing
    /// zoxide surfaces a toast and aborts.
    fn open_zoxide_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !crate::search::binary_available("zoxide") {
            self.surface_error("zoxide not installed", cx);
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let entries = crate::search::zoxide_query();
        if entries.is_empty() {
            self.surface_error("zoxide returned no results", cx);
            return;
        }
        let weak = self.workspace.clone();
        let weak_self = cx.weak_entity();
        workspace.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, move |window, cx| {
                crate::search::ZoxideModal::new(
                    entries,
                    {
                        let weak_self = weak_self.clone();
                        move |path, window, cx| {
                            // Zoxide jump is a forward navigation — push
                            // the current dir to the back stack and clear
                            // the forward stack so the user can `[` back
                            // to where they were.
                            if let Some(this) = weak_self.upgrade() {
                                this.update(cx, |fm, cx| {
                                    fm.reveal_path(path.clone(), None, window, cx);
                                });
                            }
                        }
                    },
                    weak,
                    window,
                    cx,
                )
            });
        });
    }

    /// Open the content-search modal once the user has typed a query
    /// and pressed Enter on the `ContentSearchQuery` prompt.
    fn launch_content_search(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if query.trim().is_empty() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let root = self.current_dir.clone();
        let weak = self.workspace.clone();
        workspace.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, move |window, cx| {
                crate::search::ContentSearchModal::new(root, query, weak, window, cx)
            });
        });
    }

    /// `O` (shift-o): open the choose-opener picker for the entry under
    /// the cursor (or the marked set's first entry). The picker always
    /// has at least one row — the synthetic "Codon (default)" — so this
    /// verb is never a silent no-op.
    fn choose_opener(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let targets = self.opener_targets();
        if targets.cursor.as_os_str().is_empty() {
            return;
        }
        let cursor_for_label = targets.cursor.clone();
        let label = cursor_for_label
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| cursor_for_label.display().to_string());
        let choices = crate::openers::choices_for(&targets.cursor, cx);
        let weak = self.workspace.clone();
        let fm = cx.weak_entity();
        workspace.update(cx, |ws, cx| {
            ws.toggle_modal(window, cx, move |window, cx| {
                let targets = targets.clone();
                let fm = fm.clone();
                crate::opener_picker::OpenerPickerModal::new(
                    choices,
                    label,
                    move |choice, window, cx| {
                        if let Some(this) = fm.upgrade() {
                            this.update(cx, |fm, cx| {
                                fm.run_opener_choice(choice, targets.clone(), window, cx);
                            });
                        }
                    },
                    weak,
                    window,
                    cx,
                )
            });
        });
    }

    /// Snapshot of the inputs an opener invocation needs — the cursor,
    /// the marked set, and the current directory. Captured once at the
    /// moment the picker opens so a stale picker can't be steered into
    /// running against a different cursor.
    pub(crate) fn opener_targets(&self) -> crate::opener_picker::OpenerTargets {
        let cursor = self
            .entries
            .get(self.selected_index)
            .map(|e| e.path.clone())
            .unwrap_or_else(|| self.current_dir.clone());
        let marked: Vec<PathBuf> = self
            .marked
            .iter()
            .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
            .collect();
        crate::opener_picker::OpenerTargets {
            cursor,
            marked,
            cwd: self.current_dir.clone(),
        }
    }

    /// Dispatch the picker's chosen row. `Default` falls back to the
    /// usual `workspace.open_abs_path` path; `Opener` substitutes its
    /// template against each target and routes through the existing
    /// shell-exec machinery (`block` selects blocking vs async).
    pub(crate) fn run_opener_choice(
        &mut self,
        choice: crate::openers::OpenerChoice,
        targets: crate::opener_picker::OpenerTargets,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            crate::openers::OpenerChoice::Default => {
                let paths = if targets.marked.is_empty() {
                    vec![targets.cursor]
                } else {
                    targets.marked
                };
                self.open_paths_default(paths, window, cx);
            }
            crate::openers::OpenerChoice::Opener(opener) => {
                let plan = targets.plan(&opener.cmd);
                for (cursor, marks) in plan {
                    let command = crate::shell::apply_substitutions(
                        &opener.cmd,
                        &cursor,
                        &marks,
                        &targets.cwd,
                    );
                    if command.trim().is_empty() {
                        continue;
                    }
                    self.dispatch_shell_command(
                        opener.cmd.clone(),
                        command,
                        opener.block,
                        window,
                        cx,
                    );
                }
            }
        }
    }

    /// `workspace.open_abs_path` fan-out — preserves today's behavior
    /// for files Zed knows how to open natively (text, images, …).
    /// Marked entries open in their natural workspace order; missing
    /// items log and skip.
    pub(crate) fn open_paths_default(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |_, cx| {
            for path in paths {
                let task = workspace.update_in(cx, |workspace, window, cx| {
                    workspace.open_abs_path(path, Default::default(), window, cx)
                });
                if let Ok(task) = task {
                    task.await.log_err();
                }
            }
        })
        .detach();
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
        self.apply_filter(cx);
        cx.notify();
    }

    fn apply_filter(&mut self, cx: &mut Context<Self>) {
        let Some(unfiltered) = self.entries_unfiltered.clone() else {
            return;
        };
        if self.filter_query.is_empty() {
            self.entries = unfiltered;
            self.selected_index = 0;
            self.request_preview_update(cx);
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
        self.request_preview_update(cx);
    }

    fn clear_filter(&mut self, cx: &mut Context<Self>) {
        self.filter_query.clear();
        if let Some(unfiltered) = self.entries_unfiltered.take() {
            self.entries = unfiltered;
        }
        self.selected_index = cmp::min(
            self.selected_index,
            self.entries.len().saturating_sub(1),
        );
        self.request_preview_update(cx);
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

    fn start_skip_trash_delete(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.current_targets();
        if targets.is_empty() {
            return;
        }
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::ConfirmSkipTrashDelete { targets });
        cx.notify();
    }

    fn execute_hard_delete(
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
                let is_dir = fs.is_dir(&path).await;
                let result = if is_dir {
                    fs.remove_dir(&path, options).await
                } else {
                    fs.remove_file(&path, options).await
                };
                if let Err(e) = result {
                    failures.push((path, e));
                }
            }
            this.update_in(cx, |this, window, cx| {
                for (path, e) in &failures {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    this.surface_error(format!("Couldn't delete {name}: {e}"), cx);
                }
                this.reload_entries(window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn execute_delete(
        &mut self,
        targets: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use std::sync::atomic::Ordering;
        let fs = self.fs.clone();
        let workspace = self.workspace.clone();
        let total = targets.len();
        let label = format!("Trashing {total} entries");
        let mut handle = cx.update_global::<crate::tasks::FmTaskStore, _>(|_, cx| {
            crate::tasks::begin(workspace.clone(), crate::tasks::FmTaskKind::Delete, label, total, cx)
        });
        let cancel_flag = handle.cancel_flag();
        cx.spawn_in(window, async move |this, cx| {
            let mut failures: Vec<(PathBuf, anyhow::Error)> = Vec::new();
            let mut processed = 0_usize;
            let mut cancelled = false;
            for path in targets {
                if cancel_flag.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                let options = fs::RemoveOptions {
                    recursive: true,
                    ignore_if_not_exists: false,
                };
                if let Err(e) = fs.trash(&path, options).await {
                    failures.push((path, e));
                }
                processed += 1;
                let workspace = workspace.clone();
                cx.update(|_, cx| {
                    crate::tasks::tick(&mut handle, processed, workspace, cx);
                })
                .ok();
            }
            let outcome = if cancelled {
                crate::tasks::FmTaskOutcome::Cancelled
            } else if failures.is_empty() {
                crate::tasks::FmTaskOutcome::Done
            } else {
                crate::tasks::FmTaskOutcome::Failed {
                    errors: failures
                        .iter()
                        .map(|(p, e)| format!("{}: {e}", p.display()))
                        .collect(),
                }
            };
            let workspace_finish = workspace.clone();
            cx.update(|_, cx| {
                crate::tasks::finish(handle, outcome, workspace_finish, cx);
            })
            .ok();
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
        self.reload_entries_with(None, cx);
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
                    self.clear_filter(cx);
                }
                if let Some(origin) = find_origin {
                    self.selected_index = cmp::min(
                        origin,
                        self.entries.len().saturating_sub(1),
                    );
                    self.ensure_visible();
                    self.request_preview_update(cx);
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
                    | PendingInput::Chmod { input: s, .. }
                    | PendingInput::ContentSearchQuery(s)
                    | PendingInput::ShellBlocking { input: s }
                    | PendingInput::ShellAsync { input: s } => {
                        s.pop();
                        None
                    }
                    PendingInput::Filter => {
                        self.filter_query.pop();
                        self.apply_filter(cx);
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
                    | PendingInput::ConfirmDeleteMarked { .. }
                    | PendingInput::ConfirmSkipTrashDelete { .. } => None,
                };
                if let Some((q, origin, forward)) = find_step {
                    self.apply_find_preview(&q, origin, forward, cx);
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
                // SAFETY: pending_input is Some — guarded by the let-else at
                // the top of handle_insert_key.
                let Some(pending) = self.pending_input.take() else {
                    return;
                };
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
                    | PendingInput::ConfirmDeleteMarked { .. }
                    | PendingInput::ConfirmSkipTrashDelete { .. } => {
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
                    PendingInput::ContentSearchQuery(query) => {
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        self.launch_content_search(query, window, cx);
                    }
                    PendingInput::ShellBlocking { input } if !input.trim().is_empty() => {
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        self.execute_shell_blocking(input, window, cx);
                    }
                    PendingInput::ShellAsync { input } if !input.trim().is_empty() => {
                        self.mode = PaneMode::Normal;
                        cx.notify();
                        self.execute_shell_async(input, window, cx);
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
                            | PendingInput::ConfirmSkipTrashDelete { .. }
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
                                Some(PendingInput::ConfirmSkipTrashDelete { targets }) => {
                                    self.mode = PaneMode::Normal;
                                    self.execute_hard_delete(targets, window, cx);
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
                            | PendingInput::Chmod { input: s, .. }
                            | PendingInput::ContentSearchQuery(s)
                            | PendingInput::ShellBlocking { input: s }
                            | PendingInput::ShellAsync { input: s } => {
                                s.push_str(ch);
                                None
                            }
                            PendingInput::Filter => {
                                self.filter_query.push_str(ch);
                                self.apply_filter(cx);
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
                            | PendingInput::ConfirmDeleteMarked { .. }
                            | PendingInput::ConfirmSkipTrashDelete { .. } => None,
                        };
                        if let Some((q, origin, forward)) = find_step {
                            self.apply_find_preview(&q, origin, forward, cx);
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
                'l' if !shift && !ctrl && key == "n" => {
                    self.make_symlinks(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                'l' if key == "escape" => {
                    cx.stop_propagation();
                    return;
                }
                'l' => {
                    self.enter_directory(&EnterDirectory, window, cx);
                }
                'L' if !shift && !ctrl && key == "n" => {
                    self.make_hardlinks(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                'L' if key == "escape" => {
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }

        if self.visual_anchor.is_some() {
            let extends = matches!(key, "j" | "k" | "down" | "up") && !shift && !ctrl;
            let commits = matches!(key, "escape" | "enter" | "\n");
            if !extends && !commits {
                self.visual_anchor = None;
            }
        }

        let handled = match key {
            // Navigation. Arrow keys mirror hjkl so users who haven't
            // internalized the Vim/Helix row can still drive the panel.
            "j" | "down" if !shift && !ctrl => { self.navigate_down(&NavigateDown, window, cx); true }
            "k" | "up" if !shift && !ctrl => { self.navigate_up(&NavigateUp, window, cx); true }
            // Bare `l` arms a short-window chord: if `n` lands next
            // we make symlinks; if any other key (or the timeout)
            // fires first we commit `enter_directory` instead. See
            // `arm_l_chord` for the timer logic.
            "l" | "right" if !shift && !ctrl => { self.arm_l_chord(window, cx); true }
            // `L` (shift-l) is the chord starter for hardlinks: `Ln`
            // calls `make_hardlinks`. Unlike bare `l` there is no
            // enter-directory fallback, so a plain `pending_chord`
            // entry suffices — any non-`n` key just clears it.
            "l" if shift && !ctrl => {
                self.pending_chord = Some('L');
                cx.notify();
                true
            }
            // Enter while sweeping a visual range commits the sweep
            // instead of opening the focused entry. That mirrors helix
            // and yazi behavior — the user just selected a range and
            // wouldn't expect Enter to drop them into the file.
            "enter" | "\n" if self.visual_anchor.is_some() => {
                self.commit_visual_range(cx);
                true
            }
            "enter" | "\n" => { self.enter_directory(&EnterDirectory, window, cx); true }
            "h" | "left" if !shift && !ctrl => { self.parent_directory(&ParentDirectory, window, cx); true }
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
            // `F` (shift-f): resolve the focused symlink and reveal the
            // target in its parent directory via `codon_fm::Reveal`.
            // Non-symlinks are a no-op so the binding stays cheap to
            // probe. Symlink-loop protection caps traversal at 16 hops.
            "f" if shift && !ctrl => { self.follow_symlink(window, cx); true }
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
            "f" if !shift && !ctrl => { self.start_filter(window, cx); true }
            "/" if !ctrl => { self.start_find_forward(window, cx); true }
            "?" if !ctrl => { self.start_find_backward(window, cx); true }
            "n" if !shift && !ctrl => { self.find_next(cx); true }
            "n" if shift && !ctrl => { self.find_prev(cx); true }
            "s" if !shift && !ctrl => { self.open_search_by_name(window, cx); true }
            "s" if shift && !ctrl => { self.open_search_by_content(window, cx); true }
            "z" if shift && !ctrl => { self.open_zoxide_picker(window, cx); true }
            "t" if shift && !ctrl => { self.open_trash_modal(window, cx); true }
            "x" if shift && !ctrl => { self.start_skip_trash_delete(window, cx); true }
            "o" if shift && !ctrl => { self.choose_opener(window, cx); true }
            "w" if !shift && !ctrl => { self.open_task_history(window, cx); true }
            "escape" if self.shell_running.is_some() => {
                self.terminate_shell_command(cx);
                true
            }
            "escape" if self.visual_anchor.is_some() => {
                self.commit_visual_range(cx);
                true
            }
            "escape" if !self.filter_query.is_empty() || self.entries_unfiltered.is_some() => {
                self.clear_filter(cx);
                cx.notify();
                true
            }
            // Command mode
            ";" if shift => {
                window.dispatch_action(Box::new(zed_actions::command_palette::Toggle), cx);
                true
            }
            // Shell exec
            ";" if !shift && !ctrl => { self.start_shell_async(window, cx); true }
            "!" if !ctrl => { self.start_shell_blocking(window, cx); true }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    /// `!` (blocking) — open an Insert-mode prompt seeded for the user
    /// to type a shell command. While `shell_running` is `Some`, the FM
    /// grays out and a foreground-process watcher releases it.
    fn start_shell_blocking(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.shell_running.is_some() {
            return;
        }
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::ShellBlocking {
            input: String::new(),
        });
        cx.notify();
    }

    /// `;` (async) — same prompt as `!` minus the overlay + the toast.
    /// `shell_running` is never set on this path.
    fn start_shell_async(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = PaneMode::Insert;
        self.pending_input = Some(PendingInput::ShellAsync {
            input: String::new(),
        });
        cx.notify();
    }

    /// Apply `{path}` / `{paths}` / `{name}` / `{names}` / `{cwd}` /
    /// `{parent}` against the current FM state. Used by both the
    /// blocking and async exec paths so the substitution rules stay in
    /// one place.
    fn substitute_shell_template(&self, template: &str) -> String {
        let cursor = self
            .entries
            .get(self.selected_index)
            .map(|e| e.path.clone())
            .unwrap_or_else(|| self.current_dir.clone());
        let marked: Vec<PathBuf> = self
            .marked
            .iter()
            .filter_map(|&i| self.entries.get(i).map(|e| e.path.clone()))
            .collect();
        crate::shell::apply_substitutions(template, &cursor, &marked, &self.current_dir)
    }

    /// `!` Enter path — pick (or spawn) the terminal, send the
    /// substituted command, then arm a foreground-process watcher that
    /// surfaces stderr / clears the overlay when the command exits.
    fn execute_shell_blocking(
        &mut self,
        template: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.substitute_shell_template(&template);
        if command.trim().is_empty() {
            return;
        }
        self.dispatch_shell_command(template, command, /* blocking */ true, window, cx);
    }

    /// `;` Enter path — pick (or spawn) the terminal and send the
    /// substituted command. No overlay, no watcher, no toast.
    fn execute_shell_async(
        &mut self,
        template: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = self.substitute_shell_template(&template);
        if command.trim().is_empty() {
            return;
        }
        self.dispatch_shell_command(template, command, /* blocking */ false, window, cx);
    }

    fn dispatch_shell_command(
        &mut self,
        template: String,
        command: String,
        blocking: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            self.surface_error("Workspace unavailable", cx);
            return;
        };
        let cwd = self.current_dir.clone();

        let self_id = cx.entity_id();
        let target = {
            let ws = workspace.read(cx);
            crate::shell::pick_terminal_for_shell(ws, cx, Some(self_id))
        };

        match target {
            crate::shell::TerminalTarget::Existing(view) => {
                // Reuse path: activate the pane, frame the command with
                // a cd so the user gets `cwd`-relative behavior.
                let view_clone = view.clone();
                workspace.update(cx, |workspace, cx| {
                    crate::shell::focus_terminal(workspace, &view_clone, window, cx);
                });
                crate::shell::send_to_terminal(&view, &cwd, &command, blocking, cx);
                if blocking {
                    self.arm_shell_watcher(template, command, view, window, cx);
                }
            }
            crate::shell::TerminalTarget::New => {
                // New-terminal path: spawn a fresh shell rooted at
                // `cwd`, send the command once the PTY is ready. The
                // returned Terminal entity is wrapped by the panel as a
                // TerminalView — we look it up afterward via the same
                // MRU lookup so the watcher gets the view handle.
                let cmd_for_spawn = command.clone();
                let template_for_arm = template;
                let command_for_arm = command;
                let spawn_task = workspace.update(cx, |workspace, cx| {
                    crate::shell::spawn_new_terminal_and_run(
                        workspace,
                        cwd,
                        cmd_for_spawn,
                        blocking,
                        window,
                        cx,
                    )
                });
                cx.spawn_in(window, async move |this, cx| {
                    let _terminal = spawn_task.await;
                    if !blocking {
                        return;
                    }
                    let Ok(view) = cx.update(|_, cx| {
                        workspace
                            .read(cx)
                            .recent_active_item_by_type::<terminal_view::TerminalView>(cx)
                    }) else {
                        return;
                    };
                    let Some(view) = view else { return };
                    this.update_in(cx, |this, window, cx| {
                        this.arm_shell_watcher(
                            template_for_arm,
                            command_for_arm,
                            view,
                            window,
                            cx,
                        );
                    })
                    .ok();
                })
                .detach();
            }
        }
    }

    /// Hold a polling task that compares the PTY's foreground pgrp to
    /// the shell's own pid; when they re-converge the command has
    /// exited and we drop the overlay + (if non-zero) surface stderr.
    fn arm_shell_watcher(
        &mut self,
        _template: String,
        command: String,
        view: gpui::Entity<terminal_view::TerminalView>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let weak_view = view.downgrade();
        let weak_view_for_state = weak_view.clone();
        let weak_self = cx.entity().downgrade();
        let watcher = cx.spawn(async move |_this, cx| {
            // Give the shell a moment to start the child before we look
            // for the foreground pgrp — otherwise the very first poll
            // observes the still-idle shell and the overlay closes
            // instantly.
            cx.background_executor()
                .timer(crate::shell::SHELL_POLL_INTERVAL)
                .await;
            loop {
                let still_running = cx.update(|cx| {
                    let Some(view) = weak_view.upgrade() else {
                        return false;
                    };
                    let term = view.read(cx).entity().read(cx);
                    let Some(getter) = term.pid_getter() else {
                        return false;
                    };
                    let Some(foreground) = term.pid() else {
                        return false;
                    };
                    foreground != getter.fallback_pid()
                });
                if !still_running {
                    break;
                }
                cx.background_executor()
                    .timer(crate::shell::SHELL_POLL_INTERVAL)
                    .await;
            }
            let tail = cx.update(|cx| match weak_view.upgrade() {
                Some(view) => crate::shell::snapshot_tail(&view, 8, cx),
                None => Vec::new(),
            });
            weak_self
                .update(cx, |fm, cx| {
                    fm.finish_shell_blocking(tail, cx);
                })
                .ok();
        });
        self.shell_running = Some(ShellRunState {
            command,
            terminal: weak_view_for_state,
            escape_count: 0,
            _watcher: watcher,
        });
        cx.notify();
    }

    /// Called by the watcher once the PTY foreground pgrp re-converges
    /// to the shell — i.e. the command exited. If the captured stderr
    /// tail contains a `__codon_exit_marker:N` line with `N != 0`, the
    /// last 8 lines surface via `surface_error`. Otherwise the overlay
    /// is just dismissed.
    fn finish_shell_blocking(&mut self, tail: Vec<String>, cx: &mut Context<Self>) {
        let Some(run) = self.shell_running.take() else {
            return;
        };
        let command = run.command;
        cx.notify();

        let (exit_status, body) = parse_shell_tail(&tail);

        let title: String = command.chars().take(60).collect();
        match exit_status {
            Some(0) | None => {
                // Success or unknown exit — no toast. (Unknown means we
                // couldn't parse a marker line; assume the user saw the
                // output in the terminal pane already.)
            }
            Some(_) => {
                let msg = if body.is_empty() {
                    format!("{title} (exit {})", exit_status.unwrap_or_default())
                } else {
                    format!("{title}\n{body}")
                };
                self.surface_error(msg, cx);
            }
        }
    }

    /// `Esc` while a `!` command is running:
    ///   - first press: send Ctrl-C (`\x03`) — the foreground process
    ///     group receives SIGINT through the PTY.
    ///   - second press: send Ctrl-\\ (`\x1c`, SIGQUIT) so a hung
    ///     program that ignored SIGINT still gets killed.
    /// The third press drops the overlay outright so the user can
    /// regain FM control even when the program is wedged in the kernel.
    fn terminate_shell_command(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.shell_running.as_mut() else {
            return;
        };
        run.escape_count = run.escape_count.saturating_add(1);
        let count = run.escape_count;
        if let Some(view) = run.terminal.upgrade() {
            let signal: &[u8] = match count {
                1 => b"\x03",
                _ => b"\x1c",
            };
            let bytes: Vec<u8> = signal.to_vec();
            let terminal = view.read(cx).entity().clone();
            terminal.update(cx, |term, _cx| {
                term.input(bytes);
            });
        }
        if count >= 3 {
            self.shell_running = None;
        }
        cx.notify();
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
            self.set_sort_mode(mode, window, cx);
            return;
        }
        if key == "," && !shift {
            self.toggle_sort_reverse(window, cx);
        }
    }

    fn apply_sort(&mut self, mode: crate::prefs::SortMode, cx: &mut Context<Self>) {
        self.sort = mode;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_sort(mode));
    }

    /// Shared entry point for the palette-discoverable `Sort By …`
    /// actions and the `,`-prefixed chord handler: persist the mode and
    /// re-read the directory so the new ordering takes effect.
    pub(crate) fn set_sort_mode(
        &mut self,
        mode: crate::prefs::SortMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_sort(mode, cx);
        self.reload_entries(window, cx);
    }

    pub(crate) fn toggle_sort_reverse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reverse = !self.reverse;
        let value = self.reverse;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_reverse(value));
        self.reload_entries(window, cx);
    }

    fn cycle_line_mode(&mut self, cx: &mut Context<Self>) {
        self.line_mode = self.line_mode.next();
        let mode = self.line_mode;
        cx.update_global::<crate::prefs::FmPrefs, _>(|p, _| p.set_line_mode(mode));
        cx.notify();
    }

    /// React to a modifier-only key change (no character key pressed).
    /// Used by the bottom-bar overlay: when Cmd is the only modifier
    /// held, the bar swaps its left segments for a general-shortcut
    /// row; any other modifier combination drops back to info.
    pub(crate) fn handle_modifiers_changed(
        &mut self,
        event: &gpui::ModifiersChangedEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let m = event.modifiers;
        let cmd_only = m.platform && !m.control && !m.alt && !m.shift && !m.function;
        if self.cmd_only_held != cmd_only {
            self.cmd_only_held = cmd_only;
            cx.notify();
        }
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

impl codon_mode::PaneModeBridge for FileManager {
    fn pane_mode(&self) -> PaneMode {
        // The file-manager is a Normal-mode pane: it reads as a
        // navigable list, not a text-input surface. Per-instance
        // `self.mode` transitions are an internal concern that the
        // FM tracks for its own keybinding contexts; the status-bar
        // mode indicator stays on NORMAL while the FM is focused.
        PaneMode::Normal
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
        let languages = self.language_registry.clone();
        Task::ready(Some(
            cx.new(|cx| Self::new(dir, workspace, fs, languages, window, cx)),
        ))
    }

    fn can_split(&self) -> bool {
        true
    }
}

impl SerializableItem for FileManager {
    fn serialized_item_kind() -> &'static str {
        "FileManager"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let db = FileManagerDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "file_managers", &db, cx)
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let current_dir = self.current_dir.clone();
        let db = FileManagerDb::global(cx);
        Some(cx.background_spawn(async move {
            db.save_current_dir(item_id, workspace_id, current_dir).await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(event, FileManagerEvent::PathChanged)
    }

    fn deserialize(
        _project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let db = FileManagerDb::global(cx);
        window.spawn(cx, async move |cx| {
            let stored_dir = db.get_current_dir(item_id, workspace_id).log_err().flatten();

            cx.update(|window, cx| {
                let workspace_entity = workspace
                    .upgrade()
                    .ok_or_else(|| anyhow::anyhow!("workspace was dropped before FileManager could be restored"))?;
                let (fs, languages, fallback_dir) = workspace_entity.read_with(cx, |workspace, cx| {
                    let fs = workspace.app_state().fs.clone();
                    let languages = Some(workspace.app_state().languages.clone());
                    let fallback_dir = workspace
                        .project()
                        .read(cx)
                        .worktrees(cx)
                        .next()
                        .map(|wt| wt.read(cx).abs_path().to_path_buf())
                        .unwrap_or_else(|| {
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                        });
                    (fs, languages, fallback_dir)
                });

                let dir = match stored_dir {
                    Some(p) if p.is_dir() => p,
                    _ => fallback_dir,
                };

                let weak_workspace = workspace_entity.downgrade();
                Ok(cx.new(|cx| {
                    FileManager::new(dir, weak_workspace, fs, languages, window, cx)
                }))
            })?
        })
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

/// Walk a symlink chain starting at `path`, hopping one `read_link` at
/// a time, until either a non-symlink target is reached or `max_hops`
/// have been consumed. Relative link targets are resolved against the
/// link's parent directory so the result is always absolute. Returns
/// `Err` on the first IO failure (broken link, permission denied, …)
/// or when the hop budget is exhausted — a pathological loop
/// (`a -> b -> a`) is therefore bounded, not infinite.
fn resolve_with_depth_cap(path: &Path, max_hops: usize) -> std::io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..max_hops {
        let meta = std::fs::symlink_metadata(&current)?;
        if !meta.file_type().is_symlink() {
            return Ok(current);
        }
        let raw_target = std::fs::read_link(&current)?;
        current = if raw_target.is_absolute() {
            raw_target
        } else {
            current
                .parent()
                .map(|p| p.join(&raw_target))
                .unwrap_or(raw_target)
        };
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("symlink chain exceeded {max_hops} hops"),
    ))
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

/// Inspect the last few lines of terminal output for a
/// `__codon_exit_marker:N` line emitted by the blocking-exec frame.
/// Returns the captured exit status (when present) and the
/// (newline-joined) tail with the marker line stripped — used as the
/// body of the `surface_error` toast when the exit was non-zero.
///
/// The marker is a codon-internal convention; we don't bother with PIDs
/// or correlation IDs because the terminal pane only ever runs one of
/// our blocking commands at a time (the FM blocks while it's running).
fn parse_shell_tail(tail: &[String]) -> (Option<i32>, String) {
    let mut exit_status: Option<i32> = None;
    let mut filtered: Vec<&str> = Vec::with_capacity(tail.len());
    for line in tail {
        if let Some(rest) = line.trim().strip_prefix("__codon_exit_marker:")
            && let Ok(n) = rest.parse::<i32>()
        {
            exit_status = Some(n);
            continue;
        }
        filtered.push(line.as_str());
    }
    (exit_status, filtered.join("\n"))
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
#[derive(Clone, Copy, PartialEq, Eq)]
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

/// Small per-FM LRU of recently-read directory listings, keyed on the
/// absolute path. Entries also carry the directory's `mtime` and the
/// `ReadDirOptions` they were read under — a cache hit requires the
/// stored mtime to match the directory's *current* mtime, so external
/// changes (a new file dropped in another pane, a `git pull`) bust the
/// cache transparently the next time we look the directory up.
///
/// Wrapped in `Arc<Mutex<…>>` on the `FileManager` so the background
/// listing task can consult and update it without round-tripping back
/// to the foreground.
pub(crate) struct DirCacheEntry {
    pub mtime: std::time::SystemTime,
    pub opts: ReadDirOptions,
    pub entries: Vec<DirEntry>,
}

#[derive(Default)]
pub(crate) struct DirCache {
    inner: VecDeque<(PathBuf, DirCacheEntry)>,
}

impl DirCache {
    fn lookup(
        &mut self,
        path: &Path,
        mtime: std::time::SystemTime,
        opts: ReadDirOptions,
    ) -> Option<Vec<DirEntry>> {
        let pos = self.inner.iter().position(|(p, _)| p == path)?;
        let (_, entry) = &self.inner[pos];
        if entry.mtime != mtime || entry.opts != opts {
            return None;
        }
        let result = entry.entries.clone();
        if let Some(kv) = self.inner.remove(pos) {
            self.inner.push_front(kv);
        }
        Some(result)
    }

    fn store(
        &mut self,
        path: PathBuf,
        mtime: std::time::SystemTime,
        opts: ReadDirOptions,
        entries: Vec<DirEntry>,
    ) {
        self.inner.retain(|(p, _)| p != &path);
        self.inner.push_front((
            path,
            DirCacheEntry {
                mtime,
                opts,
                entries,
            },
        ));
        while self.inner.len() > DIR_CACHE_CAP {
            self.inner.pop_back();
        }
    }
}

/// Cache-aware variant of `read_dir_sync`. Stats the directory to get
/// its current mtime, consults the cache, and falls back to a real
/// `read_dir` on miss / staleness. Returns the entries and stores the
/// fresh listing under the new mtime so subsequent lookups hit.
///
/// Designed to be called from the background executor: the cache is
/// behind a `Mutex` but contention is negligible (one FM task at a
/// time per FM instance).
fn read_dir_cached(
    cache: &std::sync::Mutex<DirCache>,
    path: &Path,
    opts: ReadDirOptions,
) -> Vec<DirEntry> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some(mtime) = mtime
        && let Ok(mut cache) = cache.lock()
        && let Some(entries) = cache.lookup(path, mtime, opts)
    {
        return entries;
    }
    let entries = read_dir_sync(path, opts);
    if let Some(mtime) = mtime
        && let Ok(mut cache) = cache.lock()
    {
        cache.store(path.to_path_buf(), mtime, opts, entries.clone());
    }
    entries
}

/// Compute the preview payload for `entry`. Pure / Send-safe so it can
/// be moved onto the background executor by `request_preview_update`.
/// All GPUI / entity construction stays on the foreground.
fn compute_preview(
    entry: &DirEntry,
    opts: ReadDirOptions,
    cache: &std::sync::Mutex<DirCache>,
) -> Preview {
    if entry.is_dir {
        let children = read_dir_cached(cache, &entry.path, opts);
        return Preview::Directory(children);
    }

    let path = entry.path.clone();
    let name = entry.name.clone();
    let size = entry.size;

    if is_image_path(&path) {
        return Preview::Image(read_image_info(&path, name, size));
    }

    if is_archive_path(&path) {
        if let Some(listing) = read_archive_listing(&path) {
            return Preview::Archive(listing);
        }
        // Recognised extension but the archive failed to open — fall
        // through to the binary fallback so the user still sees the
        // header / hex dump instead of a silent blank pane.
    }

    match read_text_preview(&path, size) {
        Some(text) => Preview::Text(text),
        None => Preview::Binary(read_binary_info(&path, name, size)),
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
            let is_dir = metadata.is_dir();
            let path = e.path();
            // `child_count` is left `None` here and filled in asynchronously
            // by `spawn_child_count_fill` after the listing paints. Doing it
            // inline costs one extra `read_dir` per subdirectory, which
            // dominated the perceived latency of entering populated dirs.
            // `labels` is built once the rest of `entry` is populated; the
            // `LineMode::Size` slot stays `None` for directories until the
            // backfill task refreshes labels for that entry.
            let mut entry = DirEntry {
                name,
                path,
                is_dir,
                is_hidden,
                is_symlink: file_type.is_symlink(),
                size: metadata.len(),
                git_status: None,
                mtime,
                btime,
                mode,
                uid,
                gid,
                child_count: None,
                labels: EntryLabels::default(),
            };
            entry.labels = build_entry_labels(&entry);
            Some(entry)
        })
        .collect();

    sort_entries(&mut entries, options.sort, options.reverse);
    entries
}

/// Precompute the per-`LineMode` meta `SharedString`s for `entry`,
/// plus the `name` label, in one pass. Called by `read_dir_sync`
/// (initial population) and `DirEntry::refresh_labels` (re-run when
/// `child_count` is backfilled by an async fill task on master).
///
/// One `SharedString` allocation per non-`None` slot, then four
/// `Arc::clone`s per row per frame instead of four `format!` + four
/// `SharedString::from(String)` allocations.
pub(crate) fn build_entry_labels(entry: &DirEntry) -> EntryLabels {
    let mut meta: [Option<gpui::SharedString>; LineMode::COUNT] = Default::default();
    for mode in [
        LineMode::None,
        LineMode::Size,
        LineMode::Mtime,
        LineMode::Permissions,
        LineMode::Owner,
    ] {
        meta[mode.idx()] = crate::view::entry_meta_label(entry, mode).map(Into::into);
    }
    EntryLabels {
        name: entry.name.clone().into(),
        meta,
    }
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

/// Upper bound on bytes read into the text preview. Above this the
/// file is treated as binary — the editor would handle a multi-megabyte
/// buffer fine, but reading the whole thing synchronously on every
/// `j`/`k` would not. Tuned so a typical source file (incl. minified
/// JS / generated code) fits comfortably.
pub(crate) const TEXT_PREVIEW_MAX_BYTES: u64 = 512 * 1024;

/// Attempt to read `path` as UTF-8 text for the preview pane. Returns
/// `None` when the file is too large, unreadable, or non-UTF-8 — the
/// caller falls back to the binary metadata renderer in that case.
pub(crate) fn read_text_preview(path: &Path, size: u64) -> Option<TextPreview> {
    if size > TEXT_PREVIEW_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let content = String::from_utf8(bytes).ok()?;
    Some(TextPreview {
        path: path.to_path_buf(),
        content,
    })
}

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
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg",
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
    register_serializable_item::<FileManager>(cx);
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
    let languages = Some(workspace.app_state().languages.clone());
    let weak_workspace = workspace.weak_handle();
    let file_manager = cx.new(|cx| {
        FileManager::new(target_dir.clone(), weak_workspace, fs, languages, window, cx)
    });
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

    let languages = Some(workspace.app_state().languages.clone());
    let file_manager =
        cx.new(|cx| FileManager::new(dir, weak_workspace, fs, languages, window, cx));
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
    fn dir_cache_lookup_hits_when_mtime_and_opts_match() {
        let mut cache = DirCache::default();
        let path = PathBuf::from("/synthetic/dir");
        let mtime = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        let opts = ReadDirOptions::default();
        let entries = vec![DirEntry {
            name: "x.txt".into(),
            path: path.join("x.txt"),
            is_dir: false,
            is_hidden: false,
            is_symlink: false,
            size: 0,
            git_status: None,
            mtime: None,
            btime: None,
            mode: None,
            uid: None,
            gid: None,
            child_count: None,
            labels: Default::default(),
        }];
        cache.store(path.clone(), mtime, opts, entries.clone());
        let hit = cache.lookup(&path, mtime, opts);
        assert_eq!(
            hit.as_deref().map(|s| s.len()),
            Some(1),
            "exact match should hit"
        );
        let stale_mtime = mtime + std::time::Duration::from_secs(1);
        assert!(
            cache.lookup(&path, stale_mtime, opts).is_none(),
            "different mtime should miss"
        );
        let other_opts = ReadDirOptions {
            show_hidden: true,
            ..opts
        };
        assert!(
            cache.lookup(&path, mtime, other_opts).is_none(),
            "different opts should miss"
        );
    }

    #[test]
    fn dir_cache_lru_evicts_oldest() {
        let mut cache = DirCache::default();
        let mtime = std::time::SystemTime::UNIX_EPOCH;
        let opts = ReadDirOptions::default();
        // Fill past the cap.
        for i in 0..(DIR_CACHE_CAP + 2) {
            cache.store(
                PathBuf::from(format!("/dir/{i}")),
                mtime,
                opts,
                Vec::new(),
            );
        }
        // Earliest two paths must have been evicted.
        assert!(cache.lookup(&PathBuf::from("/dir/0"), mtime, opts).is_none());
        assert!(cache.lookup(&PathBuf::from("/dir/1"), mtime, opts).is_none());
        // Most recent ones still resolve.
        assert!(
            cache
                .lookup(
                    &PathBuf::from(format!("/dir/{}", DIR_CACHE_CAP + 1)),
                    mtime,
                    opts
                )
                .is_some()
        );
    }

    #[test]
    fn dir_cache_mtime_change_busts_the_cache() {
        let dir = make_tree(&[("a.txt", false)]);
        let cache = std::sync::Mutex::new(DirCache::default());
        let opts = ReadDirOptions::default();
        let _ = read_dir_cached(&cache, dir.path(), opts);
        // Sleep ~10 ms then create a file: this bumps the directory's
        // mtime (POSIX), so the next lookup must miss.
        std::thread::sleep(std::time::Duration::from_millis(15));
        fs::write(dir.path().join("b.txt"), b"").expect("touch");
        let after = read_dir_cached(&cache, dir.path(), opts);
        assert_eq!(
            after.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["a.txt", "b.txt"]
        );
    }

    #[test]
    fn dir_cache_different_opts_miss_each_other() {
        let dir = make_tree(&[
            ("visible.txt", false),
            (".hidden.txt", false),
        ]);
        let cache = std::sync::Mutex::new(DirCache::default());
        let no_hidden = ReadDirOptions::default();
        let with_hidden = ReadDirOptions {
            show_hidden: true,
            ..ReadDirOptions::default()
        };
        let a = read_dir_cached(&cache, dir.path(), no_hidden);
        assert_eq!(
            a.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["visible.txt"]
        );
        let b = read_dir_cached(&cache, dir.path(), with_hidden);
        assert_eq!(
            b.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec![".hidden.txt", "visible.txt"]
        );
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
        assert!(is_image_path(Path::new("foo.svg")));
        assert!(is_image_path(Path::new("foo.SVG")));
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

    #[test]
    fn parse_shell_tail_extracts_exit_status_and_filters_marker() {
        let tail = vec![
            "error: something went wrong".to_string(),
            "stack frame".to_string(),
            "__codon_exit_marker:1".to_string(),
        ];
        let (status, body) = parse_shell_tail(&tail);
        assert_eq!(status, Some(1));
        assert_eq!(body, "error: something went wrong\nstack frame");
    }

    #[test]
    fn parse_shell_tail_returns_none_when_no_marker() {
        let tail = vec!["hello".to_string(), "world".to_string()];
        let (status, body) = parse_shell_tail(&tail);
        assert!(status.is_none());
        assert_eq!(body, "hello\nworld");
    }

    #[test]
    fn parse_shell_tail_handles_success_marker() {
        let tail = vec![
            "build complete".to_string(),
            "__codon_exit_marker:0".to_string(),
        ];
        let (status, body) = parse_shell_tail(&tail);
        assert_eq!(status, Some(0));
        assert_eq!(body, "build complete");
    }
}
