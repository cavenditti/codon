use std::sync::Arc;

use codon_pickers::{ModalModeTag, ModalScaffold};
use fuzzy::StringMatchCandidate;
use gpui::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement, Render, Styled, Subscription, Task, Window,
};
use picker::{Picker, PickerDelegate};
use ui::{
    HighlightedLabel, LabelCommon, ListItem, ListItemSpacing, Toggleable as _, h_flex, v_flex,
};
use workspace::{ModalView, Workspace};

use crate::{
    registry::SessionRegistry,
    session::{Session, SessionId},
};

#[derive(Clone, Debug)]
struct SessionSelected {
    id: SessionId,
}

#[derive(Clone, Debug)]
struct PickerDismissed;

pub struct SessionPickerDelegate {
    selected_index: usize,
    candidates: Vec<Session>,
    matches: Vec<fuzzy::StringMatch>,
}

impl SessionPickerDelegate {
    fn new(cx: &App) -> Self {
        let candidates = SessionRegistry::global(cx).sessions();
        Self {
            selected_index: 0,
            matches: (0..candidates.len())
                .map(|ix| fuzzy::StringMatch {
                    candidate_id: ix,
                    score: 0.0,
                    positions: Vec::new(),
                    string: candidates[ix].name.clone(),
                })
                .collect(),
            candidates,
        }
    }
}

impl EventEmitter<SessionSelected> for Picker<SessionPickerDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<SessionPickerDelegate> {}

impl PickerDelegate for SessionPickerDelegate {
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
        Arc::from("Switch session…")
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
            .map(|(ix, s)| StringMatchCandidate::new(ix, &s.name))
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

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(session) = self.candidates.get(matched.candidate_id) else {
            return;
        };
        cx.emit(SessionSelected { id: session.id });
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
        let session = self.candidates.get(matched.candidate_id)?;
        Some(
            ListItem::new(ix)
                .toggle_state(selected)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    h_flex().gap_3().child(
                        v_flex()
                            .child(HighlightedLabel::new(
                                matched.string.clone(),
                                matched.positions.clone(),
                            ))
                            .child(
                                ui::Label::new(session.cwd.display().to_string())
                                    .color(ui::Color::Muted)
                                    .size(ui::LabelSize::Small),
                            ),
                    ),
                ),
        )
    }
}

pub struct SessionSwitchModal {
    scaffold: ModalScaffold,
    picker: Entity<Picker<SessionPickerDelegate>>,
    _subscriptions: [Subscription; 2],
}

impl SessionSwitchModal {
    pub fn new(
        workspace: gpui::WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = SessionPickerDelegate::new(cx);
        let picker =
            cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &SessionSelected, window, cx| {
                let id = event.id;
                if let Some(workspace) = workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        crate::actions::attach_session(workspace, id, window, cx);
                    });
                }
                this.dismiss(window, cx);
            },
        );

        let on_dismiss =
            cx.subscribe_in(&picker, window, |this, _, _: &PickerDismissed, window, cx| {
                this.dismiss(window, cx);
            });

        Self {
            scaffold,
            picker,
            _subscriptions: [on_select, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for SessionSwitchModal {}
impl EventEmitter<DismissEvent> for SessionSwitchModal {}

impl Focusable for SessionSwitchModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for SessionSwitchModal {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w_96().child(self.picker.clone())
    }
}

#[cfg(test)]
mod compile_assertions {
    use crate::actions;

    // Compile-time assertion: `actions::SessionSwitch` is the action
    // type the SessionSwitch picker reacts to. Catches accidental
    // removal of the type while the picker still references it.
    #[allow(dead_code)]
    fn assert_session_switch_action(_: &actions::SessionSwitch) {}
}
