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

use crate::{registry::SessionRegistry, session::WindowId, window_indicator::switch_to_window};

#[derive(Clone, Debug)]
struct WindowSelected {
    id: WindowId,
}

#[derive(Clone, Debug)]
struct PickerDismissed;

#[derive(Clone)]
struct WindowCandidate {
    id: WindowId,
    name: String,
    summary: String,
}

pub struct WindowPickerDelegate {
    selected_index: usize,
    candidates: Vec<WindowCandidate>,
    matches: Vec<fuzzy::StringMatch>,
}

impl WindowPickerDelegate {
    fn new(cx: &App) -> Self {
        let candidates: Vec<WindowCandidate> = SessionRegistry::global(cx)
            .active()
            .map(|s| {
                s.windows
                    .iter()
                    .map(|w| WindowCandidate {
                        id: w.id,
                        name: w.name.clone(),
                        summary: format!("window {}", w.id.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
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

impl EventEmitter<WindowSelected> for Picker<WindowPickerDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<WindowPickerDelegate> {}

impl PickerDelegate for WindowPickerDelegate {
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
        Arc::from("Switch window…")
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

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(candidate) = self.candidates.get(matched.candidate_id) else {
            return;
        };
        cx.emit(WindowSelected { id: candidate.id });
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
                                ui::Label::new(candidate.summary.clone())
                                    .color(ui::Color::Muted)
                                    .size(ui::LabelSize::Small),
                            ),
                    ),
                ),
        )
    }
}

pub struct WindowSwitchModal {
    scaffold: ModalScaffold,
    picker: Entity<Picker<WindowPickerDelegate>>,
    _subscriptions: [Subscription; 2],
}

impl WindowSwitchModal {
    pub fn new(
        workspace: gpui::WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = WindowPickerDelegate::new(cx);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_select = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &WindowSelected, window, cx| {
                let target = event.id;
                if let Some(ws) = workspace.upgrade() {
                    ws.update(cx, |ws, cx| {
                        switch_to_window(ws, target, window, cx);
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

impl ModalView for WindowSwitchModal {}
impl EventEmitter<DismissEvent> for WindowSwitchModal {}

impl Focusable for WindowSwitchModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for WindowSwitchModal {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w_96().child(self.picker.clone())
    }
}
