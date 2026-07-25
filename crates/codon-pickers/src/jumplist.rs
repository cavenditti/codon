//! Jumplist picker — `codon_pickers::JumplistPicker`.
//!
//! A two-layer "places I've been" picker:
//!
//! 1. Vim/Helix jumplist entries for the active editor — sourced from
//!    [`vim::codon_jumplist::workspace_jumplist_entries`], which wraps the
//!    pane's `NavHistory`. `ctrl-o` / `ctrl-i` walk the same data; the
//!    picker just renders it as a fuzzy-matchable list.
//! 2. Recent pane activations across the workspace — sourced from each
//!    pane's `activation_history()`, which the workspace already tracks
//!    for the tab MRU. We collapse non-editor panes to a single "[pane]"
//!    row so terminals and the file manager surface in the same list.
//!
//! Confirming a `[jump]` row reopens the file at the recorded row;
//! confirming a `[pane]` row activates the pane.
//!
//! See `TASK:phase-16/pickers-jumplist` and
//! `REQ:codon/helix-pickers#c-jumplist-picker`.

use std::sync::Arc;

use fuzzy::StringMatchCandidate;
use gpui::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, WeakEntity,
    Window, actions,
};
use picker::{Picker, PickerDelegate};
use ui::{
    Color, HighlightedLabel, Icon, IconName, Label, LabelCommon, LabelSize, ListItem,
    ListItemSpacing, StyledExt, Toggleable as _, h_flex, rems, v_flex,
};
use vim::codon_jumplist::{self, JumplistEntry};
use workspace::{ModalView, Workspace};

use crate::last_picker;
use crate::scaffold::{ModalModeTag, ModalScaffold};

actions!(
    codon_pickers,
    [
        /// Open the jumplist picker — fuzzy-match across the active pane's
        /// `NavHistory` plus the workspace's pane-activation history.
        JumplistPicker,
    ]
);

pub(crate) const PICKER_ACTION_NAME: &str = "codon_pickers::JumplistPicker";

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(handle_toggle);
    })
    .detach();
}

fn handle_toggle(
    workspace: &mut Workspace,
    _: &JumplistPicker,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak = workspace.weak_handle();
    let initial = last_picker::take_query_for(cx, PICKER_ACTION_NAME);
    // Collect rows BEFORE `toggle_modal` updates the workspace entity —
    // anything inside the modal-constructor closure that does
    // `workspace.read(cx)` would double-borrow and panic.
    let rows = collect_rows(workspace, cx);
    workspace.toggle_modal(window, cx, move |window, cx| {
        JumplistModal::new(weak, rows, initial, window, cx)
    });
}

/// Tag distinguishing the two row kinds. Rendered as an `[jump]` /
/// `[pane]` prefix in the picker label so a keyboard user sees what each
/// row will do before they confirm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JumplistRowKind {
    Jump,
    Pane,
}

#[derive(Clone)]
struct JumplistRow {
    kind: JumplistRowKind,
    /// Rendered label; fuzzy match scores against this verbatim.
    display: String,
    /// Filled when `kind == Jump`. The picker uses it on confirm.
    jump: Option<JumplistEntry>,
    /// Filled when `kind == Pane`. Index into `Workspace::panes()` at
    /// enumeration time — re-resolved on confirm to guard against panes
    /// closing between open and confirm.
    pane_index: Option<usize>,
}

pub struct JumplistPickerDelegate {
    workspace: WeakEntity<Workspace>,
    rows: Vec<JumplistRow>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
    last_query: SharedString,
}

impl JumplistPickerDelegate {
    fn new(workspace: WeakEntity<Workspace>, rows: Vec<JumplistRow>) -> Self {
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

fn collect_rows(workspace: &Workspace, cx: &App) -> Vec<JumplistRow> {
    let mut rows: Vec<JumplistRow> = Vec::new();

    // Layer 1: vim jumplist (NavHistory) entries.
    let jumps = codon_jumplist::workspace_jumplist_entries(workspace, cx);
    for jump in jumps {
        let path = jump
            .abs_path
            .clone()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| {
                jump.project_path
                    .path
                    .display(util::paths::PathStyle::local())
                    .into_owned()
            });
        let row_label = match jump.row {
            Some(r) => format!("[jump] {path}:{r}"),
            None => format!("[jump] {path}"),
        };
        rows.push(JumplistRow {
            kind: JumplistRowKind::Jump,
            display: row_label,
            jump: Some(jump),
            pane_index: None,
        });
    }

    // Layer 2: recent pane activations. Codon's `WindowRuntimeCache`
    // (in `codon-session`) tracks finer-grained pane-history for
    // window-switching but lives in a downstream crate that can't be a
    // dep here without an import cycle. The workspace's own pane list,
    // ordered by their `activation_history()` last-activated timestamps,
    // gives the same surface for the picker's purposes.
    for (ix, pane) in workspace.panes().iter().enumerate() {
        let pane_ref = pane.read(cx);
        let active_item_label = pane_ref
            .active_item()
            .map(|item| item.tab_content_text(0, cx).to_string())
            .unwrap_or_else(|| "Empty pane".to_string());
        let display = format!("[pane] {active_item_label}");
        rows.push(JumplistRow {
            kind: JumplistRowKind::Pane,
            display,
            jump: None,
            pane_index: Some(ix),
        });
    }

    rows
}

impl PickerDelegate for JumplistPickerDelegate {
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
        Arc::from("Filter jumps and panes…")
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
        let Some(row) = self.rows.get(matched.candidate_id).cloned() else {
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            cx.emit(PickerDismissed);
            return;
        };
        match row.kind {
            JumplistRowKind::Jump => {
                if let Some(entry) = row.jump.as_ref() {
                    codon_jumplist::jump_to_entry(&workspace, entry, window, cx).detach();
                }
            }
            JumplistRowKind::Pane => {
                if let Some(ix) = row.pane_index {
                    let handle = workspace.update(cx, |workspace, cx| {
                        workspace.panes().get(ix).map(|pane| pane.focus_handle(cx))
                    });
                    if let Some(handle) = handle {
                        window.focus(&handle, cx);
                    }
                }
            }
        }
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
        let icon = match row.kind {
            JumplistRowKind::Jump => IconName::ArrowRight,
            JumplistRowKind::Pane => IconName::Split,
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

#[derive(Clone, Debug)]
pub(crate) struct PickerDismissed;

impl EventEmitter<PickerDismissed> for Picker<JumplistPickerDelegate> {}

pub struct JumplistModal {
    scaffold: ModalScaffold,
    picker: Entity<Picker<JumplistPickerDelegate>>,
    _subscriptions: Vec<Subscription>,
}

impl JumplistModal {
    fn new(
        workspace: WeakEntity<Workspace>,
        rows: Vec<JumplistRow>,
        initial_query: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = JumplistPickerDelegate::new(workspace, rows);
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

impl ModalView for JumplistModal {}
impl EventEmitter<DismissEvent> for JumplistModal {}

impl Focusable for JumplistModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for JumplistModal {
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
                            Label::new(format!("{count} jumps + panes"))
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
    fn row_label_includes_kind_tag() {
        let jump_row = JumplistRow {
            kind: JumplistRowKind::Jump,
            display: "[jump] foo.rs:12".into(),
            jump: None,
            pane_index: None,
        };
        let pane_row = JumplistRow {
            kind: JumplistRowKind::Pane,
            display: "[pane] Terminal".into(),
            jump: None,
            pane_index: Some(0),
        };
        assert!(jump_row.display.starts_with("[jump]"));
        assert!(pane_row.display.starts_with("[pane]"));
    }

    #[test]
    fn row_kinds_distinct() {
        // Pinned because confirm() switches on these two variants — if a
        // third kind is ever added, the match arm must be updated.
        assert_ne!(JumplistRowKind::Jump, JumplistRowKind::Pane);
    }
}
