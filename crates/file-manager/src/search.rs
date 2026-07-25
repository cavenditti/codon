use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
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

use crate::Reveal;

/// Hard cap on the candidate set across the phase-7 search pickers. The
/// fuzzy matcher already prunes display, but capping the input keeps the
/// picker responsive when `fd` / walkdir emit tens of thousands of hits.
pub(crate) const MAX_CANDIDATES: usize = 5000;

/// Used by the `walkdir` fallback for name search. The cap is intentionally
/// the same as `MAX_CANDIDATES` so the toast message stays accurate.
const WALKDIR_CAP: usize = MAX_CANDIDATES;

#[derive(Clone, Debug)]
struct NameMatch {
    /// Absolute path to the entry.
    abs_path: PathBuf,
    /// Path string shown in the picker — `abs_path` relative to the search
    /// root when possible, full absolute path otherwise.
    display: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PathSelected {
    pub(crate) abs_path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct PickerDismissed;

/// State for the name-search picker. The async producer task pushes
/// candidates into `candidates`; the delegate reads them on every
/// `update_matches` call and refilters via `fuzzy::match_strings`.
struct NameSearchDelegate {
    root: PathBuf,
    /// Most recent query string the user typed — kept so newly arrived
    /// candidates can be merged into the visible list using the same
    /// fuzzy ranking.
    query: String,
    candidates: Vec<NameMatch>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
}

impl NameSearchDelegate {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            query: String::new(),
            candidates: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
        }
    }

    /// Append a batch of candidates from the producer task. Called on the
    /// main thread so it can safely mutate state and request a re-match.
    fn append_batch(
        &mut self,
        batch: Vec<NameMatch>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        if batch.is_empty() {
            return;
        }
        let remaining = MAX_CANDIDATES.saturating_sub(self.candidates.len());
        let take = remaining.min(batch.len());
        if take == 0 {
            return;
        }
        self.candidates.extend(batch.into_iter().take(take));
        let query = self.query.clone();
        self.update_matches(query, window, cx).detach();
    }
}

impl EventEmitter<PathSelected> for Picker<NameSearchDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<NameSearchDelegate> {}

impl PickerDelegate for NameSearchDelegate {
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
        Arc::from("Search by name…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.query = query.clone();
        let candidates: Vec<StringMatchCandidate> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(ix, c)| StringMatchCandidate::new(ix, &c.display))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = AtomicBool::new(false);
        cx.spawn(async move |this, cx| {
            let matches = fuzzy::match_strings(
                &candidates,
                query.trim(),
                false,
                true,
                200,
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

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(candidate) = self.candidates.get(matched.candidate_id) else {
            return;
        };
        cx.emit(PathSelected {
            abs_path: candidate.abs_path.clone(),
        });
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
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
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex()
                        .flex_grow()
                        .gap_3()
                        .child(Icon::new(IconName::File).color(Color::Muted))
                        .child(HighlightedLabel::new(
                            matched.string.clone(),
                            matched.positions.clone(),
                        )),
                ),
        )
    }
}

pub struct NameSearchModal {
    picker: Entity<Picker<NameSearchDelegate>>,
    _task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl NameSearchModal {
    pub fn new(
        root: PathBuf,
        _workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = NameSearchDelegate::new(root.clone());
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &PathSelected, window, cx| {
                window.dispatch_action(Box::new(Reveal(event.abs_path.clone())), cx);
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

        let task = spawn_name_producer(root, picker.downgrade(), window, cx);
        Self {
            picker,
            _task: task,
            _subscriptions: vec![on_select, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for NameSearchModal {}
impl EventEmitter<DismissEvent> for NameSearchModal {}
impl Focusable for NameSearchModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for NameSearchModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = self.picker.read(cx).delegate.root.display().to_string();
        div()
            .elevation_3(cx)
            .w(rems(42.))
            .flex_1()
            .overflow_hidden()
            .child(
                v_flex()
                    .child(
                        h_flex().px_3().py_1().child(
                            Label::new(format!("name: {root}"))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.picker.clone()),
            )
    }
}

/// Spawn either the `fd`-backed streaming producer or the walkdir
/// fallback. Both push `NameMatch` batches into the picker's delegate via
/// `update_matches`.
fn spawn_name_producer(
    root: PathBuf,
    weak_picker: gpui::WeakEntity<Picker<NameSearchDelegate>>,
    window: &mut Window,
    cx: &mut Context<NameSearchModal>,
) -> Task<()> {
    let has_fd = which("fd").is_some() || which("fdfind").is_some();
    if has_fd {
        cx.spawn_in(window, async move |_modal, cx| {
            let batches = cx.background_spawn(async move { run_fd(&root) }).await;
            for batch in batches {
                weak_picker
                    .update_in(cx, |picker, window, cx| {
                        picker.delegate.append_batch(batch, window, cx);
                    })
                    .ok();
            }
        })
    } else {
        cx.spawn_in(window, async move |_modal, cx| {
            let (batch, _truncated) = cx.background_spawn(async move { run_walkdir(&root) }).await;
            weak_picker
                .update_in(cx, |picker, window, cx| {
                    picker.delegate.append_batch(batch, window, cx);
                })
                .ok();
        })
    }
}

/// Synchronously run `fd` and return one batch per ~64 lines so the
/// picker can refresh incrementally. We don't stream stdout line-by-line
/// because the picker can't accept events from a background task without
/// holding the main `Context`; chunked batches strike a workable middle.
fn run_fd(root: &Path) -> Vec<Vec<NameMatch>> {
    let bin = if which("fd").is_some() {
        "fd"
    } else {
        "fdfind"
    };
    let output = std::process::Command::new(bin)
        .args(["--type", "f", "--type", "d", "--hidden", "--no-ignore"])
        .arg(".")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut batches: Vec<Vec<NameMatch>> = Vec::new();
    let mut batch: Vec<NameMatch> = Vec::with_capacity(64);
    let mut count = 0usize;
    for line in stdout.lines() {
        if count >= MAX_CANDIDATES {
            break;
        }
        let trimmed = line.trim_end_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        let rel = PathBuf::from(trimmed);
        let abs_path = if rel.is_absolute() {
            rel.clone()
        } else {
            root.join(&rel)
        };
        let display = rel.display().to_string();
        batch.push(NameMatch { abs_path, display });
        count += 1;
        if batch.len() == 64 {
            batches.push(std::mem::take(&mut batch));
            batch = Vec::with_capacity(64);
        }
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// Walkdir fallback when `fd` is not installed. Synchronous, depth-first,
/// capped at `WALKDIR_CAP` entries. Returns `(matches, truncated)` so
/// callers can surface a toast when the listing was clipped.
fn run_walkdir(root: &Path) -> (Vec<NameMatch>, bool) {
    let mut out: Vec<NameMatch> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut truncated = false;
    while let Some(dir) = stack.pop() {
        if out.len() >= WALKDIR_CAP {
            truncated = true;
            break;
        }
        let Ok(reader) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in reader.flatten() {
            if out.len() >= WALKDIR_CAP {
                truncated = true;
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let display = rel.display().to_string();
            if display.is_empty() {
                continue;
            }
            out.push(NameMatch {
                abs_path: path.clone(),
                display,
            });
            if file_type.is_dir() {
                stack.push(path);
            }
        }
    }
    (out, truncated)
}

/// Cross-platform `which`-style lookup. Returns the first matching binary
/// path on `$PATH`, or `None` when not found.
pub(crate) fn which(bin: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Whether `name` is callable. Used by the search pickers' keymaps to
/// bail out before opening a modal that would have nothing to show.
pub fn binary_available(bin: &str) -> bool {
    which(bin).is_some()
}

#[derive(Clone, Debug)]
struct ContentHit {
    abs_path: PathBuf,
    line: u32,
    column: u32,
    /// `<relative-path>:<line>:<col>  <snippet>` — the row text shown in
    /// the picker. Highlight offsets index into this string.
    display: String,
    /// Byte offset within `display` where the matched substring begins.
    match_start: usize,
    /// Byte offset within `display` where the matched substring ends.
    match_end: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ContentSelected {
    pub(crate) abs_path: PathBuf,
    pub(crate) row: u32,
    pub(crate) column: u32,
}

struct ContentSearchDelegate {
    root: PathBuf,
    query: String,
    candidates: Vec<ContentHit>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
}

impl ContentSearchDelegate {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            query: String::new(),
            candidates: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
        }
    }

    fn append_batch(
        &mut self,
        batch: Vec<ContentHit>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        if batch.is_empty() {
            return;
        }
        let remaining = MAX_CANDIDATES.saturating_sub(self.candidates.len());
        let take = remaining.min(batch.len());
        if take == 0 {
            return;
        }
        self.candidates.extend(batch.into_iter().take(take));
        self.matches = self
            .candidates
            .iter()
            .enumerate()
            .map(|(ix, c)| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: (c.match_start..c.match_end).collect(),
                string: c.display.clone(),
            })
            .collect();
        if self.selected_index >= self.matches.len() {
            self.selected_index = 0;
        }
        cx.notify();
        let query = self.query.clone();
        self.update_matches(query, window, cx).detach();
    }
}

impl EventEmitter<ContentSelected> for Picker<ContentSearchDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<ContentSearchDelegate> {}

impl PickerDelegate for ContentSearchDelegate {
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
        Arc::from("Filter results…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.query = query.clone();
        // Empty filter — show the raw rg-ordered hits with their
        // submatch highlights preserved.
        if query.trim().is_empty() {
            self.matches = self
                .candidates
                .iter()
                .enumerate()
                .map(|(ix, c)| fuzzy::StringMatch {
                    candidate_id: ix,
                    score: 0.0,
                    positions: (c.match_start..c.match_end).collect(),
                    string: c.display.clone(),
                })
                .collect();
            if self.selected_index >= self.matches.len() {
                self.selected_index = 0;
            }
            cx.notify();
            return Task::ready(());
        }

        let candidates: Vec<StringMatchCandidate> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(ix, c)| StringMatchCandidate::new(ix, &c.display))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = AtomicBool::new(false);
        cx.spawn(async move |this, cx| {
            let matches = fuzzy::match_strings(
                &candidates,
                query.trim(),
                false,
                true,
                200,
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

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(candidate) = self.candidates.get(matched.candidate_id) else {
            return;
        };
        cx.emit(ContentSelected {
            abs_path: candidate.abs_path.clone(),
            row: candidate.line,
            column: candidate.column,
        });
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
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
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex()
                        .flex_grow()
                        .gap_3()
                        .child(Icon::new(IconName::FileCode).color(Color::Muted))
                        .child(HighlightedLabel::new(
                            matched.string.clone(),
                            matched.positions.clone(),
                        )),
                ),
        )
    }
}

pub struct ContentSearchModal {
    picker: Entity<Picker<ContentSearchDelegate>>,
    _task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ContentSearchModal {
    pub fn new(
        root: PathBuf,
        query: String,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = ContentSearchDelegate::new(root.clone());
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &ContentSelected, window, cx| {
                let path = event.abs_path.clone();
                let row = event.row;
                let col = event.column;
                if let Some(ws) = workspace.upgrade() {
                    let task = ws.update(cx, |ws, cx| {
                        ws.open_abs_path(
                            path.clone(),
                            workspace::OpenOptions::default(),
                            window,
                            cx,
                        )
                    });
                    cx.spawn_in(window, async move |_modal, cx| {
                        let opened = task.await.ok();
                        if let Some(item) = opened {
                            cx.update(|window, cx| {
                                go_to_position(item, row, col, window, cx);
                            })
                            .ok();
                        }
                    })
                    .detach();
                }
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

        let task = spawn_content_producer(root, query, picker.downgrade(), window, cx);
        Self {
            picker,
            _task: task,
            _subscriptions: vec![on_select, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for ContentSearchModal {}
impl EventEmitter<DismissEvent> for ContentSearchModal {}
impl Focusable for ContentSearchModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for ContentSearchModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (root, query) = {
            let delegate = &self.picker.read(cx).delegate;
            (delegate.root.display().to_string(), delegate.query.clone())
        };
        let header = if query.is_empty() {
            format!("content: {root}")
        } else {
            format!("content: {root}  /{query}/")
        };
        div()
            .elevation_3(cx)
            .w(rems(60.))
            .flex_1()
            .overflow_hidden()
            .child(
                v_flex()
                    .child(
                        h_flex().px_3().py_1().child(
                            Label::new(header)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.picker.clone()),
            )
    }
}

/// Move the editor's cursor to (row, col), where both are 1-based as
/// ripgrep emits them. No-op on items that aren't editors.
fn go_to_position(
    item: Box<dyn workspace::item::ItemHandle>,
    row: u32,
    col: u32,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(editor) = item.act_as::<editor::Editor>(cx) else {
        return;
    };
    let row = row.saturating_sub(1);
    let col = col.saturating_sub(1);
    editor.update(cx, |editor, cx| {
        let Some(buffer) = editor.buffer().read(cx).as_singleton() else {
            return;
        };
        let snapshot = buffer.read(cx).snapshot();
        let point = snapshot.point_from_external_input(row, col);
        editor.go_to_singleton_buffer_point(point, window, cx);
    });
}

fn spawn_content_producer(
    root: PathBuf,
    query: String,
    weak_picker: gpui::WeakEntity<Picker<ContentSearchDelegate>>,
    window: &mut Window,
    cx: &mut Context<ContentSearchModal>,
) -> Task<()> {
    cx.spawn_in(window, async move |_modal, cx| {
        let batches = cx
            .background_spawn(async move { run_ripgrep(&root, &query) })
            .await;
        for batch in batches {
            weak_picker
                .update_in(cx, |picker, window, cx| {
                    picker.delegate.append_batch(batch, window, cx);
                })
                .ok();
            // Yield so the picker can paint before we push the next chunk.
            cx.background_executor()
                .timer(Duration::from_millis(1))
                .await;
        }
    })
}

/// Run ripgrep with `--json` and parse the streamed output into
/// `ContentHit` rows. Output is chunked into ~32-row batches so the
/// picker repaints while results are still arriving.
fn run_ripgrep(root: &Path, query: &str) -> Vec<Vec<ContentHit>> {
    if query.is_empty() {
        return Vec::new();
    }
    let output = std::process::Command::new("rg")
        .args([
            "--json",
            "--max-count=50",
            "--max-columns=200",
            "--no-heading",
            "--",
            query,
        ])
        .arg(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut batches: Vec<Vec<ContentHit>> = Vec::new();
    let mut batch: Vec<ContentHit> = Vec::with_capacity(32);
    let mut count = 0usize;
    for line in stdout.lines() {
        if count >= MAX_CANDIDATES {
            break;
        }
        let Some(hit) = parse_rg_line(line, root) else {
            continue;
        };
        batch.push(hit);
        count += 1;
        if batch.len() == 32 {
            batches.push(std::mem::take(&mut batch));
            batch = Vec::with_capacity(32);
        }
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// Parse a single `rg --json` line into a `ContentHit`. Returns `None`
/// for non-match events (begin/end/summary). rg's match-event shape is
/// stable and narrow, so we use a hand-rolled traversal of the parsed
/// `serde_json::Value` rather than declaring a dedicated struct.
fn parse_rg_line(line: &str, root: &Path) -> Option<ContentHit> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "match" {
        return None;
    }
    let data = v.get("data")?;
    let path_text = data.get("path")?.get("text")?.as_str()?.to_string();
    let abs_path = PathBuf::from(&path_text);
    let line_no = data.get("line_number")?.as_u64()? as u32;
    let lines_text = data.get("lines")?.get("text")?.as_str()?;
    let trimmed_line = lines_text.trim_end_matches('\n');

    let submatches = data.get("submatches")?.as_array()?;
    let first = submatches.first()?;
    let match_text = first.get("match")?.get("text")?.as_str()?.to_string();
    let start_byte = first.get("start")?.as_u64()? as usize;
    let column = start_byte as u32 + 1;

    let rel = abs_path
        .strip_prefix(root)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| abs_path.clone());
    let header = format!("{}:{line_no}:{column}  ", rel.display());
    let highlight_start = header.len() + start_byte;
    let highlight_end = highlight_start + match_text.len();
    let display = format!("{header}{trimmed_line}");
    // Guard against malformed offsets — the display string is what we
    // index into, so anything out of range is silently truncated rather
    // than crashing the picker.
    let highlight_end = highlight_end.min(display.len());
    let highlight_start = highlight_start.min(highlight_end);

    Some(ContentHit {
        abs_path,
        line: line_no,
        column,
        display,
        match_start: highlight_start,
        match_end: highlight_end,
    })
}

#[derive(Clone, Debug)]
struct ZoxideEntry {
    abs_path: PathBuf,
    display: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ZoxidePicked {
    pub(crate) abs_path: PathBuf,
}

struct ZoxideDelegate {
    candidates: Vec<ZoxideEntry>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
    query: String,
}

impl ZoxideDelegate {
    fn new(entries: Vec<ZoxideEntry>) -> Self {
        let matches = entries
            .iter()
            .enumerate()
            .map(|(ix, e)| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: Vec::new(),
                string: e.display.clone(),
            })
            .collect();
        Self {
            candidates: entries,
            matches,
            selected_index: 0,
            query: String::new(),
        }
    }
}

impl EventEmitter<ZoxidePicked> for Picker<ZoxideDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<ZoxideDelegate> {}

impl PickerDelegate for ZoxideDelegate {
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
        Arc::from("Jump (zoxide)…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.query = query.clone();
        let candidates: Vec<StringMatchCandidate> = self
            .candidates
            .iter()
            .enumerate()
            .map(|(ix, c)| StringMatchCandidate::new(ix, &c.display))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = AtomicBool::new(false);
        cx.spawn(async move |this, cx| {
            let matches = fuzzy::match_strings(
                &candidates,
                query.trim(),
                false,
                true,
                200,
                &cancel,
                executor,
            )
            .await;
            this.update(cx, |picker, cx| {
                if query.trim().is_empty() {
                    // Empty query preserves zoxide's score ordering by
                    // re-emitting candidates in their original sequence.
                    picker.delegate.matches = picker
                        .delegate
                        .candidates
                        .iter()
                        .enumerate()
                        .map(|(ix, c)| fuzzy::StringMatch {
                            candidate_id: ix,
                            score: 0.0,
                            positions: Vec::new(),
                            string: c.display.clone(),
                        })
                        .collect();
                } else {
                    picker.delegate.matches = matches;
                }
                if picker.delegate.selected_index >= picker.delegate.matches.len() {
                    picker.delegate.selected_index = 0;
                }
                cx.notify();
            })
            .ok();
        })
    }

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(candidate) = self.candidates.get(matched.candidate_id) else {
            return;
        };
        cx.emit(ZoxidePicked {
            abs_path: candidate.abs_path.clone(),
        });
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
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
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex()
                        .flex_grow()
                        .gap_3()
                        .child(Icon::new(IconName::Folder).color(Color::Muted))
                        .child(HighlightedLabel::new(
                            matched.string.clone(),
                            matched.positions.clone(),
                        )),
                ),
        )
    }
}

pub struct ZoxideModal {
    picker: Entity<Picker<ZoxideDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl ZoxideModal {
    /// `on_pick` is invoked with the chosen path when the user presses
    /// Enter. Caller is responsible for navigating the file manager to
    /// that directory.
    pub fn new<F>(
        entries: Vec<PathBuf>,
        on_pick: F,
        _workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        F: Fn(PathBuf, &mut Window, &mut App) + 'static,
    {
        let entries = entries
            .into_iter()
            .map(|p| {
                let display = p.display().to_string();
                ZoxideEntry {
                    abs_path: p,
                    display,
                }
            })
            .collect::<Vec<_>>();
        let delegate = ZoxideDelegate::new(entries);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &ZoxidePicked, window, cx| {
                on_pick(event.abs_path.clone(), window, cx);
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
            picker,
            _subscriptions: vec![on_select, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for ZoxideModal {}
impl EventEmitter<DismissEvent> for ZoxideModal {}
impl Focusable for ZoxideModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for ZoxideModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .elevation_3(cx)
            .w(rems(42.))
            .flex_1()
            .overflow_hidden()
            .child(
                v_flex()
                    .child(
                        h_flex().px_3().py_1().child(
                            Label::new("zoxide")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.picker.clone()),
            )
    }
}

/// Run `zoxide query -l` and return the list of paths, highest-scored
/// first. Returns an empty vec if zoxide is missing or the command
/// fails — callers are expected to check `binary_available("zoxide")`
/// first and surface a toast for the missing-binary case.
pub fn zoxide_query() -> Vec<PathBuf> {
    let output = std::process::Command::new("zoxide")
        .args(["query", "-l"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| PathBuf::from(line.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn run_walkdir_lists_files_and_dirs_under_root() {
        let dir = TempDir::new().expect("create tempdir");
        fs::write(dir.path().join("a.txt"), b"").expect("touch a");
        fs::create_dir(dir.path().join("sub")).expect("mkdir sub");
        fs::write(dir.path().join("sub/b.txt"), b"").expect("touch b");
        let (matches, truncated) = run_walkdir(dir.path());
        assert!(!truncated);
        let displays: Vec<&str> = matches.iter().map(|m| m.display.as_str()).collect();
        assert!(displays.contains(&"a.txt"));
        assert!(displays.contains(&"sub"));
        assert!(displays.contains(&"sub/b.txt"));
    }

    #[test]
    fn run_walkdir_not_truncated_on_small_tree() {
        let dir = TempDir::new().expect("create tempdir");
        for i in 0..10 {
            fs::write(dir.path().join(format!("f{i}.txt")), b"").expect("touch");
        }
        let (_matches, truncated) = run_walkdir(dir.path());
        assert!(!truncated);
    }

    #[test]
    fn parse_rg_line_extracts_path_line_col_and_match() {
        let root = Path::new("/tmp/proj");
        let line = r#"{"type":"match","data":{"path":{"text":"/tmp/proj/foo.rs"},"lines":{"text":"hello world\n"},"line_number":3,"absolute_offset":0,"submatches":[{"match":{"text":"world"},"start":6,"end":11}]}}"#;
        let hit = parse_rg_line(line, root).expect("parsed");
        assert_eq!(hit.line, 3);
        assert_eq!(hit.column, 7);
        assert!(hit.display.starts_with("foo.rs:3:7  "));
        assert!(hit.display.contains("hello world"));
        let highlighted = &hit.display[hit.match_start..hit.match_end];
        assert_eq!(highlighted, "world");
    }

    #[test]
    fn parse_rg_line_ignores_non_match_events() {
        let root = Path::new("/tmp/proj");
        let begin = r#"{"type":"begin","data":{"path":{"text":"/tmp/proj/x.rs"}}}"#;
        let summary = r#"{"type":"summary","data":{"elapsed_total":{"human":"0.01s"}}}"#;
        assert!(parse_rg_line(begin, root).is_none());
        assert!(parse_rg_line(summary, root).is_none());
    }

    #[test]
    fn zoxide_query_returns_empty_when_missing() {
        if which("zoxide").is_none() {
            assert!(zoxide_query().is_empty());
        }
    }
}
