//! `codon_agent::TraceViewer` — picker over the [`TraceLog`] ring
//! buffer (REQ:codon/agent-harness#c-trace), newest turn first.
//!
//! Keyboard-first: arrows / fuzzy-filter to select, Enter yanks the
//! selected turn's pretty-printed trace (metadata only — the trace
//! never holds message bodies) to the system clipboard, Esc dismisses.
//! Bound to `prefix a t` in the default keymap.

use std::sync::Arc;

use codon_pickers::{ModalModeTag, ModalScaffold};
use fuzzy::StringMatchCandidate;
use gpui::{
    App, AppContext as _, ClipboardItem, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, Styled, Subscription, Task, Window,
};
use picker::{Picker, PickerDelegate};
use ui::{
    HighlightedLabel, LabelCommon, ListItem, ListItemSpacing, Toggleable as _, h_flex, v_flex,
};
use workspace::{ModalView, Workspace};

use crate::runtime::{PhaseEvent, TraceLog, TraceOutcome, TurnTrace};

#[derive(Clone, Debug)]
struct TraceYanked;

#[derive(Clone, Debug)]
struct PickerDismissed;

struct TraceCandidate {
    label: String,
    summary: String,
    pretty: String,
}

fn outcome_label(outcome: &TraceOutcome) -> String {
    match outcome {
        TraceOutcome::InFlight => "in flight".to_string(),
        TraceOutcome::Ok { stop, turns } => match stop {
            Some(stop) => format!("ok ({stop}, {turns} turns)"),
            None => format!("ok ({turns} turns)"),
        },
        TraceOutcome::Cancelled => "cancelled".to_string(),
        TraceOutcome::TooManyTurns { limit } => format!("hit turn limit ({limit})"),
        TraceOutcome::Error { kind } => format!("error: {kind}"),
    }
}

fn duration_ms(trace: &TurnTrace) -> u64 {
    trace
        .phases
        .iter()
        .map(|phase| match phase {
            PhaseEvent::PreambleBuilt { at_ms, .. }
            | PhaseEvent::ModelCallStarted { at_ms, .. }
            | PhaseEvent::ModelCallFinished { at_ms, .. }
            | PhaseEvent::Cancelled { at_ms } => *at_ms,
        })
        .max()
        .unwrap_or(0)
}

fn candidate(trace: &TurnTrace) -> TraceCandidate {
    let label = format!(
        "#{} {} — {}",
        trace.id,
        trace.agent,
        outcome_label(&trace.outcome)
    );
    let summary = format!(
        "{} · {} tool calls · {} ms · ↓ {} ↑ {} tokens",
        trace.model,
        trace.tools.len(),
        duration_ms(trace),
        trace.tokens_in,
        trace.tokens_out,
    );
    TraceCandidate {
        label,
        summary,
        pretty: trace.pretty(),
    }
}

pub struct TraceViewerDelegate {
    selected_index: usize,
    candidates: Vec<TraceCandidate>,
    matches: Vec<fuzzy::StringMatch>,
}

impl TraceViewerDelegate {
    fn new(cx: &App) -> Self {
        let candidates: Vec<TraceCandidate> = TraceLog::entries(cx).iter().map(candidate).collect();
        Self {
            selected_index: 0,
            matches: (0..candidates.len())
                .map(|ix| fuzzy::StringMatch {
                    candidate_id: ix,
                    score: 0.0,
                    positions: Vec::new(),
                    string: candidates[ix].label.clone(),
                })
                .collect(),
            candidates,
        }
    }
}

impl EventEmitter<TraceYanked> for Picker<TraceViewerDelegate> {}
impl EventEmitter<PickerDismissed> for Picker<TraceViewerDelegate> {}

impl PickerDelegate for TraceViewerDelegate {
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
        if self.candidates.is_empty() {
            Arc::from("No agent turns recorded yet…")
        } else {
            Arc::from("Filter agent turns (Enter yanks the trace)…")
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
            .map(|(ix, c)| StringMatchCandidate::new(ix, &c.label))
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

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(candidate) = self.candidates.get(matched.candidate_id) else {
            return;
        };
        // Yank rather than open-in-buffer: keyboard-first, zero project
        // plumbing, and the trace is metadata-only so the clipboard is a
        // safe sink. An open-in-read-only-buffer variant can layer on
        // once a scratch-buffer helper exists.
        cx.write_to_clipboard(ClipboardItem::new_string(candidate.pretty.clone()));
        cx.emit(TraceYanked);
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

pub struct TraceViewerModal {
    scaffold: ModalScaffold,
    picker: Entity<Picker<TraceViewerDelegate>>,
    _subscriptions: [Subscription; 2],
}

impl TraceViewerModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scaffold = ModalScaffold::new(cx, ModalModeTag::Inert);
        scaffold.on_open(cx);
        cx.on_release(|this: &mut Self, cx| this.scaffold.on_dismiss(cx))
            .detach();
        let delegate = TraceViewerDelegate::new(cx);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_yank = cx.subscribe_in(&picker, window, |this, _, _: &TraceYanked, window, cx| {
            this.dismiss(window, cx);
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
            _subscriptions: [on_yank, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for TraceViewerModal {}
impl EventEmitter<DismissEvent> for TraceViewerModal {}

impl Focusable for TraceViewerModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for TraceViewerModal {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w_96().child(self.picker.clone())
    }
}

/// Open the trace viewer modal over `workspace`.
pub fn toggle(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    workspace.toggle_modal(window, cx, TraceViewerModal::new);
}
