use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use fuzzy::StringMatchCandidate;
use gpui::{
    AnyElement, App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render, SharedString,
    Styled, Subscription, Task, WeakEntity, Window,
};
use picker::{Picker, PickerDelegate};
use ui::{
    Color, HighlightedLabel, Label, LabelCommon, LabelSize, ListItem, ListItemSpacing,
    Toggleable as _, h_flex, v_flex,
};
use workspace::ModalView;

use crate::file_manager::FileManager;

#[derive(Clone)]
pub(crate) struct TrashRow {
    pub(crate) original_path: PathBuf,
    pub(crate) display: String,
    #[cfg(not(target_os = "macos"))]
    pub(crate) item: Arc<trash::TrashItem>,
}

#[derive(Clone, Debug)]
pub(crate) struct TrashOperationCompleted;

#[derive(Clone, Debug)]
pub(crate) struct PickerDismissed;

impl EventEmitter<TrashOperationCompleted> for Picker<TrashPickerDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<TrashPickerDelegate> {}

pub(crate) struct TrashPickerDelegate {
    selected_index: usize,
    rows: Vec<TrashRow>,
    matches: Vec<fuzzy::StringMatch>,
    marked: BTreeSet<usize>,
    message: Option<String>,
    loaded: bool,
    confirm_purge: bool,
}

impl TrashPickerDelegate {
    fn empty() -> Self {
        Self {
            selected_index: 0,
            rows: Vec::new(),
            matches: Vec::new(),
            marked: BTreeSet::new(),
            message: None,
            loaded: false,
            confirm_purge: false,
        }
    }

    fn set_rows(&mut self, rows: Vec<TrashRow>) {
        self.matches = (0..rows.len())
            .map(|ix| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: Vec::new(),
                string: rows[ix].display.clone(),
            })
            .collect();
        self.rows = rows;
        self.selected_index = 0;
        self.marked.clear();
        self.loaded = true;
    }

    fn set_message(&mut self, msg: impl Into<String>) {
        self.message = Some(msg.into());
    }

    fn clear_message(&mut self) {
        self.message = None;
    }

    /// The candidate ids of every row that the next confirm should act
    /// on — either the marked set, or the highlighted row when nothing
    /// is marked. Candidate ids index into `rows`.
    fn current_targets(&self) -> Vec<usize> {
        if !self.marked.is_empty() {
            self.marked.iter().copied().collect()
        } else if let Some(matched) = self.matches.get(self.selected_index) {
            vec![matched.candidate_id]
        } else {
            Vec::new()
        }
    }

    fn toggle_mark_at_cursor(&mut self) -> bool {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return false;
        };
        let id = matched.candidate_id;
        if !self.marked.insert(id) {
            self.marked.remove(&id);
        }
        true
    }

    #[cfg(not(target_os = "macos"))]
    fn restore(&mut self, target_ids: Vec<usize>, cx: &mut Context<Picker<Self>>) {
        let items: Vec<trash::TrashItem> = target_ids
            .iter()
            .filter_map(|id| self.rows.get(*id).map(|row| (*row.item).clone()))
            .collect();
        if items.is_empty() {
            return;
        }
        let count = items.len();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { trash::os_limited::restore_all(items) })
                .await;
            this.update(cx, |picker, cx| match result {
                Ok(()) => {
                    log::info!("file-manager: restored {count} trash entries");
                    cx.emit(TrashOperationCompleted);
                }
                Err(err) => {
                    picker
                        .delegate
                        .set_message(format!("Couldn't restore: {err}"));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn restore(&mut self, _target_ids: Vec<usize>, cx: &mut Context<Picker<Self>>) {
        self.set_message("trash restore is not supported on macOS");
        cx.notify();
    }

    #[cfg(not(target_os = "macos"))]
    fn purge(&mut self, target_ids: Vec<usize>, cx: &mut Context<Picker<Self>>) {
        let items: Vec<trash::TrashItem> = target_ids
            .iter()
            .filter_map(|id| self.rows.get(*id).map(|row| (*row.item).clone()))
            .collect();
        if items.is_empty() {
            return;
        }
        let count = items.len();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { trash::os_limited::purge_all(items) })
                .await;
            this.update(cx, |picker, cx| match result {
                Ok(()) => {
                    log::info!("file-manager: purged {count} trash entries");
                    cx.emit(TrashOperationCompleted);
                }
                Err(err) => {
                    picker
                        .delegate
                        .set_message(format!("Couldn't purge: {err}"));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn purge(&mut self, _target_ids: Vec<usize>, cx: &mut Context<Picker<Self>>) {
        self.set_message("trash purge is not supported on macOS");
        cx.notify();
    }
}

#[cfg(not(target_os = "macos"))]
fn load_trash_rows() -> anyhow::Result<Vec<TrashRow>> {
    let items = trash::os_limited::list()?;
    let mut rows: Vec<TrashRow> = items
        .into_iter()
        .map(|item| {
            let original_path = item.original_path();
            let display = original_path.display().to_string();
            TrashRow {
                original_path,
                display,
                item: Arc::new(item),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.display.to_lowercase().cmp(&b.display.to_lowercase()));
    Ok(rows)
}

#[cfg(target_os = "macos")]
fn load_trash_rows() -> anyhow::Result<Vec<TrashRow>> {
    anyhow::bail!("trash listing is not supported on macOS")
}

impl PickerDelegate for TrashPickerDelegate {
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
        Arc::from("Filter trash by original path…")
    }

    fn no_matches_text(&self, _window: &mut Window, _cx: &mut App) -> Option<SharedString> {
        if !self.loaded {
            Some(SharedString::from("Loading trash…"))
        } else if let Some(msg) = &self.message {
            Some(SharedString::from(msg.clone()))
        } else if self.rows.is_empty() {
            Some(SharedString::from("Trash is empty."))
        } else {
            Some(SharedString::from("No matching trashed items."))
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
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| StringMatchCandidate::new(ix, &row.display))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = std::sync::atomic::AtomicBool::new(false);

        cx.spawn(async move |this, cx| {
            let matches = if query.is_empty() {
                (0..candidates.len())
                    .map(|ix| fuzzy::StringMatch {
                        candidate_id: ix,
                        score: 0.0,
                        positions: Vec::new(),
                        string: candidates[ix].string.to_string(),
                    })
                    .collect()
            } else {
                fuzzy::match_strings(
                    &candidates,
                    &query,
                    false,
                    true,
                    100,
                    &cancel,
                    executor,
                )
                .await
            };
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
        let targets = self.current_targets();
        if targets.is_empty() {
            return;
        }
        if self.confirm_purge {
            self.confirm_purge = false;
            self.purge(targets, cx);
        } else {
            self.restore(targets, cx);
        }
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(PickerDismissed);
    }

    fn render_header(
        &self,
        _window: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        if !self.confirm_purge {
            return None;
        }
        let count = if self.marked.is_empty() { 1 } else { self.marked.len() };
        let prompt = format!(
            "Permanently delete {count} item{}? Enter / y to confirm, anything else cancels.",
            if count == 1 { "" } else { "s" },
        );
        Some(
            h_flex()
                .px_2p5()
                .py_1()
                .child(
                    Label::new(prompt)
                        .color(Color::Error)
                        .size(LabelSize::Small),
                )
                .into_any_element(),
        )
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let matched = self.matches.get(ix)?;
        let row = self.rows.get(matched.candidate_id)?;
        let is_marked = self.marked.contains(&matched.candidate_id);
        let mark_indicator: AnyElement = if is_marked {
            Label::new("[*]")
                .size(LabelSize::Small)
                .color(Color::Accent)
                .into_any_element()
        } else {
            Label::new("   ")
                .size(LabelSize::Small)
                .color(Color::Muted)
                .into_any_element()
        };
        let name = row
            .original_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| row.display.clone());
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex().gap_2().child(mark_indicator).child(
                        v_flex()
                            .child(HighlightedLabel::new(
                                matched.string.clone(),
                                matched.positions.clone(),
                            ))
                            .child(
                                Label::new(name)
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            ),
                    ),
                ),
        )
    }
}

pub struct TrashModal {
    picker: Entity<Picker<TrashPickerDelegate>>,
    file_manager: WeakEntity<FileManager>,
    _subscriptions: [Subscription; 2],
}

impl TrashModal {
    pub fn new(
        file_manager: WeakEntity<FileManager>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = TrashPickerDelegate::empty();
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        cx.spawn(async move |this, cx| {
            let load_result = cx
                .background_executor()
                .spawn(async move { load_trash_rows() })
                .await;
            this.update(cx, |this, cx| {
                this.picker.update(cx, |picker, cx| {
                    match load_result {
                        Ok(rows) => picker.delegate.set_rows(rows),
                        Err(err) => {
                            picker.delegate.loaded = true;
                            picker
                                .delegate
                                .set_message(format!("Couldn't read trash: {err}"));
                        }
                    }
                    cx.notify();
                });
            })
            .ok();
        })
        .detach();

        let on_completed = cx.subscribe_in(
            &picker,
            window,
            |this, _, _: &TrashOperationCompleted, window, cx| {
                if let Some(fm) = this.file_manager.upgrade() {
                    fm.update(cx, |fm, cx| fm.reload_entries_after_bulk_rename(cx));
                }
                this.dismiss(window, cx);
            },
        );
        let on_dismissed =
            cx.subscribe_in(&picker, window, |this, _, _: &PickerDismissed, window, cx| {
                this.dismiss(window, cx);
            });

        Self {
            picker,
            file_manager,
            _subscriptions: [on_completed, on_dismissed],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn intercept_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let shift = event.keystroke.modifiers.shift;
        let ctrl = event.keystroke.modifiers.control;
        let alt = event.keystroke.modifiers.alt;
        let cmd = event.keystroke.modifiers.platform;
        if ctrl || alt || cmd {
            return;
        }

        let pending_confirm = self
            .picker
            .read(cx)
            .delegate
            .confirm_purge;
        if pending_confirm {
            match key {
                "y" | "enter" | "\n" if !shift => {
                    self.picker.update(cx, |picker, cx| {
                        let targets = picker.delegate.current_targets();
                        picker.delegate.confirm_purge = false;
                        if !targets.is_empty() {
                            picker.delegate.purge(targets, cx);
                        }
                        cx.notify();
                    });
                    cx.stop_propagation();
                    return;
                }
                _ => {
                    self.picker.update(cx, |picker, cx| {
                        picker.delegate.confirm_purge = false;
                        cx.notify();
                    });
                    cx.stop_propagation();
                    return;
                }
            }
        }

        match key {
            "space" if !shift => {
                self.picker.update(cx, |picker, cx| {
                    if picker.delegate.toggle_mark_at_cursor() {
                        picker.delegate.clear_message();
                        cx.notify();
                    }
                });
                cx.stop_propagation();
            }
            "x" if shift => {
                self.picker.update(cx, |picker, cx| {
                    if !picker.delegate.current_targets().is_empty() {
                        picker.delegate.confirm_purge = true;
                        picker.delegate.clear_message();
                        cx.notify();
                    }
                });
                cx.stop_propagation();
            }
            _ => {
                let _ = window;
            }
        }
    }
}

impl ModalView for TrashModal {}
impl EventEmitter<DismissEvent> for TrashModal {}

impl Focusable for TrashModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for TrashModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_96()
            .capture_key_down(cx.listener(Self::intercept_key))
            .child(self.picker.clone())
    }
}
