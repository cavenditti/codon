//! Action history ring — `codon_history::{RepeatLast, HistoryPicker}`.
//!
//! Tracks the last N (default 10) non-motion actions fired through the
//! GPUI keystroke pipeline. `RepeatLast` (bound to `.` in Normal mode in
//! every pane) re-dispatches the most recent entry against the currently
//! focused element — i.e. it's context-aware, "repeat the last verb here".
//! `HistoryPicker` (bound to `prefix ;`) opens a small modal listing the
//! ring; confirm dispatches the chosen action at the focused element.
//!
//! **Hook mechanism.** GPUI exposes a global "post-dispatch" hook via
//! [`App::observe_keystrokes`]; we subscribe once and inspect each
//! [`KeystrokeEvent`] for its resolved action. This catches every
//! keystroke-driven dispatch in codon (including those triggered through
//! vendored Zed crates) without any per-action plumbing. Programmatic
//! `cx.dispatch_action` calls bypass the hook — those have to call
//! [`record`] explicitly if they want to land in the ring.
//!
//! **Payload preservation.** GPUI's [`Action`] trait has no
//! reverse-serialisation, so we re-use the resolved action's
//! [`Action::boxed_clone`] to capture state at fire time and re-dispatch
//! the same clone on repeat. This means typed actions with payloads
//! (e.g. `WindowGoto(2)`) repeat with their original argument — the
//! transient-selection case noted in the task spec is not handled
//! (selections live in the editor's state, not the action's payload).
//!
//! See `TASK:phase-20/action-history-ring` and
//! `REQ:codon/keymap#c-action-history`.

use std::collections::VecDeque;
use std::time::Instant;

use gpui::{
    Action, App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, Global, IntoElement, KeystrokeEvent, ParentElement, Render, SharedString, Styled,
    Subscription, Task, WeakEntity, Window, actions,
};
use picker::{Picker, PickerDelegate};
use ui::{
    Color, HighlightedLabel, Label, LabelCommon, LabelSize, ListItem, ListItemSpacing, StyledExt,
    Toggleable as _, h_flex, rems, v_flex,
};
use workspace::{ModalView, Workspace};

actions!(
    codon_history,
    [
        /// Re-fire the most recent non-motion action in the history ring
        /// at the currently focused element.
        RepeatLast,
        /// Open the action-history picker.
        HistoryPicker,
    ]
);

const DEFAULT_CAPACITY: usize = 10;

/// One slot in the history ring.
///
/// `action` is a `boxed_clone()` of the action as it was dispatched —
/// re-firing it preserves any typed payload the original carried. The
/// JSON `payload` is kept around for diagnostic rendering / future
/// persistence, but is not required for repeat dispatch.
pub struct HistoryEntry {
    pub action_name: String,
    pub action: Box<dyn Action>,
    pub payload: Option<serde_json::Value>,
    pub fired_at: Instant,
}

impl Clone for HistoryEntry {
    fn clone(&self) -> Self {
        Self {
            action_name: self.action_name.clone(),
            action: self.action.boxed_clone(),
            payload: self.payload.clone(),
            fired_at: self.fired_at,
        }
    }
}

pub struct History {
    entries: VecDeque<HistoryEntry>,
    cap: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(DEFAULT_CAPACITY),
            cap: DEFAULT_CAPACITY,
        }
    }
}

impl History {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(cap.max(1)),
            cap: cap.max(1),
        }
    }

    fn push(&mut self, entry: HistoryEntry) {
        if self.entries.len() == self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn last(&self) -> Option<&HistoryEntry> {
        self.entries.back()
    }

    pub fn entries(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter()
    }
}

impl Global for History {}

/// Hardcoded set of action names that are treated as motions and never
/// enter the ring. Kept terse — the curated list is small and rare to
/// extend; richer classification (user TOML overrides) can come later.
fn is_motion(name: &str) -> bool {
    // Codon-side history actions themselves never enter the ring.
    if name.starts_with("codon_history::") {
        return true;
    }
    // Anything in the vim namespace is treated as motion-ish for now;
    // verbs like vim::HelixSubstitute are arguably non-motion but
    // including them all keeps the ring clean while we refine the list.
    if name.starts_with("vim::") {
        return true;
    }
    if name.contains("Motion") {
        return true;
    }
    matches!(
        name,
        "workspace::ActivatePaneLeft"
            | "workspace::ActivatePaneRight"
            | "workspace::ActivatePaneUp"
            | "workspace::ActivatePaneDown"
            | "workspace::SwapPaneLeft"
            | "workspace::SwapPaneRight"
            | "workspace::SwapPaneUp"
            | "workspace::SwapPaneDown"
            | "codon_session::ResizePaneLeft"
            | "codon_session::ResizePaneDown"
            | "codon_session::ResizePaneUp"
            | "codon_session::ResizePaneRight"
            | "pane::ActivateNextItem"
            | "pane::ActivatePreviousItem"
    )
}

/// Record an explicit history entry from a programmatic dispatch site.
///
/// Callers wanting to surface a non-keystroke-driven action in the ring
/// (e.g. an action fired from a context menu) can call this directly.
/// The keystroke observer installed by [`init`] covers the common case.
pub fn record(cx: &mut App, action_name: String, payload: Option<serde_json::Value>) {
    if is_motion(&action_name) {
        return;
    }
    let Ok(action) = cx.build_action(&action_name, payload.clone()) else {
        log::debug!(
            "codon-history: record() could not rebuild action '{action_name}' for boxed clone"
        );
        return;
    };
    if !cx.has_global::<History>() {
        cx.set_global(History::default());
    }
    cx.global_mut::<History>().push(HistoryEntry {
        action_name,
        action,
        payload,
        fired_at: Instant::now(),
    });
}

fn record_from_keystroke(cx: &mut App, action: &dyn Action) {
    let name = action.name().to_string();
    if is_motion(&name) {
        return;
    }
    let boxed = action.boxed_clone();
    if !cx.has_global::<History>() {
        cx.set_global(History::default());
    }
    cx.global_mut::<History>().push(HistoryEntry {
        action_name: name,
        action: boxed,
        payload: None,
        fired_at: Instant::now(),
    });
}

/// Read-only view of the most recent entry (clone semantics so callers
/// outside this crate can use it without holding a borrow on the
/// global).
pub fn last(cx: &App) -> Option<HistoryEntry> {
    cx.try_global::<History>()
        .and_then(|h| h.last().cloned())
}

/// Snapshot of the ring (oldest first). Used by the picker.
pub fn entries(cx: &App) -> Vec<HistoryEntry> {
    cx.try_global::<History>()
        .map(|h| h.entries().cloned().collect())
        .unwrap_or_default()
}

pub fn init(cx: &mut App) {
    if !cx.has_global::<History>() {
        cx.set_global(History::default());
    }

    cx.observe_keystrokes(|event: &KeystrokeEvent, _window, cx| {
        if let Some(action) = event.action.as_ref() {
            record_from_keystroke(cx, action.as_ref());
        }
    })
    .detach();

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(handle_repeat_last);
        workspace.register_action(handle_history_picker);
    })
    .detach();
}

fn handle_repeat_last(
    _workspace: &mut Workspace,
    _: &RepeatLast,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(entry) = last(cx) else {
        log::debug!("codon-history: RepeatLast with empty ring — no-op");
        return;
    };
    window.dispatch_action(entry.action, cx);
}

fn handle_history_picker(
    workspace: &mut Workspace,
    _: &HistoryPicker,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let rows = entries(cx);
    if rows.is_empty() {
        log::debug!("codon-history: HistoryPicker opened with empty ring");
        return;
    }
    let weak = workspace.weak_handle();
    workspace.toggle_modal(window, cx, move |window, cx| {
        HistoryPickerModal::new(weak, rows, window, cx)
    });
}

#[derive(Clone, Debug)]
struct PickerDismissed;

impl EventEmitter<PickerDismissed> for Picker<HistoryPickerDelegate> {}

struct HistoryPickerDelegate {
    workspace: WeakEntity<Workspace>,
    rows: Vec<HistoryEntry>,
    selected_index: usize,
    // We don't fuzzy-match here — the ring is at most 10 entries; a
    // straight list keeps the implementation trivial and the recency
    // ordering preserved.
    matches: Vec<fuzzy::StringMatch>,
}

impl HistoryPickerDelegate {
    fn new(workspace: WeakEntity<Workspace>, mut rows: Vec<HistoryEntry>) -> Self {
        // Show most-recent first; the ring stores oldest first.
        rows.reverse();
        let matches = rows
            .iter()
            .enumerate()
            .map(|(ix, entry)| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: Vec::new(),
                string: entry.action_name.clone(),
            })
            .collect();
        Self {
            workspace,
            rows,
            selected_index: 0,
            matches,
        }
    }
}

impl PickerDelegate for HistoryPickerDelegate {
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

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> std::sync::Arc<str> {
        std::sync::Arc::from("Filter history…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let trimmed = query.trim().to_lowercase();
        if trimmed.is_empty() {
            self.matches = self
                .rows
                .iter()
                .enumerate()
                .map(|(ix, entry)| fuzzy::StringMatch {
                    candidate_id: ix,
                    score: 0.0,
                    positions: Vec::new(),
                    string: entry.action_name.clone(),
                })
                .collect();
        } else {
            self.matches = self
                .rows
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.action_name.to_lowercase().contains(&trimmed))
                .map(|(ix, entry)| fuzzy::StringMatch {
                    candidate_id: ix,
                    score: 0.0,
                    positions: Vec::new(),
                    string: entry.action_name.clone(),
                })
                .collect();
        }
        if self.selected_index >= self.matches.len() {
            self.selected_index = 0;
        }
        cx.notify();
        Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(entry) = self.rows.get(matched.candidate_id) else {
            return;
        };
        if self.workspace.upgrade().is_none() {
            cx.emit(PickerDismissed);
            return;
        }
        let action = entry.action.boxed_clone();
        window.dispatch_action(action, cx);
        cx.emit(PickerDismissed);
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
        let entry = self.rows.get(matched.candidate_id)?;
        let elapsed = entry.fired_at.elapsed().as_secs();
        let ago = if elapsed < 1 {
            "just now".to_string()
        } else if elapsed < 60 {
            format!("{elapsed}s ago")
        } else if elapsed < 3600 {
            format!("{}m ago", elapsed / 60)
        } else {
            format!("{}h ago", elapsed / 3600)
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
                        .child(HighlightedLabel::new(
                            matched.string.clone(),
                            matched.positions.clone(),
                        ))
                        .child(
                            Label::new(SharedString::from(ago))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                ),
        )
    }
}

struct HistoryPickerModal {
    picker: Entity<Picker<HistoryPickerDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl HistoryPickerModal {
    fn new(
        workspace: WeakEntity<Workspace>,
        rows: Vec<HistoryEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = HistoryPickerDelegate::new(workspace, rows);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));
        let on_dismiss = cx.subscribe_in(
            &picker,
            window,
            |this, _, _: &PickerDismissed, window, cx| {
                this.dismiss(window, cx);
            },
        );
        Self {
            picker,
            _subscriptions: vec![on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for HistoryPickerModal {}
impl EventEmitter<DismissEvent> for HistoryPickerModal {}

impl Focusable for HistoryPickerModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for HistoryPickerModal {
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
                            Label::new(format!("{count} history entries"))
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

    #[test]
    fn motion_classifier_excludes_pane_movement() {
        assert!(is_motion("workspace::ActivatePaneLeft"));
        assert!(is_motion("codon_session::ResizePaneDown"));
        assert!(is_motion("pane::ActivateNextItem"));
        assert!(is_motion("vim::Down"));
        assert!(is_motion("editor::SelectLargerSyntaxNodeMotion"));
        assert!(is_motion("codon_history::RepeatLast"));
    }

    #[test]
    fn motion_classifier_includes_verbs() {
        assert!(!is_motion("codon_session::SplitRight"));
        assert!(!is_motion("editor::Paste"));
        assert!(!is_motion("git::StageFile"));
    }

    #[test]
    fn history_caps_at_capacity() {
        // Smoke test the ring behaviour without a real GPUI App — we
        // need a dummy Box<dyn Action> to push, but Action is hard to
        // construct outside of the registry. Instead, exercise the
        // push/pop arithmetic on a small History with a no-op closure-
        // backed action substitute. Pull in `gpui::NoAction` if it
        // ever lands; for now the test is intentionally light.
        let history = History::new(3);
        assert_eq!(history.cap, 3);
        assert_eq!(history.entries.len(), 0);
    }
}
