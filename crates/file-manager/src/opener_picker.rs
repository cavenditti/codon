//! `O` opener picker.
//!
//! Mirrors the codon search-modal shape (`search::NameSearchModal`): a
//! `Picker<Delegate>` wrapped in a `ModalView`, emitting a
//! domain-specific event on confirm and a `DismissEvent` on cancel.
//! The picker rows are pre-computed at construction (the opener set is
//! tiny — usually <10 entries — so fuzzy filtering over a single line
//! per row stays cheap).

use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

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

use crate::openers::OpenerChoice;

/// Emitted when the user confirms a row. The receiver looks at `choice`
/// to decide between "run this opener" and "fall through to
/// `workspace.open_abs_path`".
#[derive(Clone, Debug)]
pub(crate) struct OpenerConfirmed {
    pub(crate) choice: OpenerChoice,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenerPickerDismissed;

struct OpenerPickerDelegate {
    choices: Vec<OpenerChoice>,
    /// `<description> (<cmd>)` strings — matched against the user's query.
    /// Held alongside `choices` so `fuzzy::match_strings` can drive
    /// `StringMatchCandidate` construction directly.
    labels: Vec<String>,
    matches: Vec<fuzzy::StringMatch>,
    selected_index: usize,
}

impl OpenerPickerDelegate {
    fn new(choices: Vec<OpenerChoice>) -> Self {
        let labels: Vec<String> = choices.iter().map(|c| c.label()).collect();
        let matches: Vec<fuzzy::StringMatch> = labels
            .iter()
            .enumerate()
            .map(|(ix, label)| fuzzy::StringMatch {
                candidate_id: ix,
                score: 0.0,
                positions: Vec::new(),
                string: label.clone(),
            })
            .collect();
        Self {
            choices,
            labels,
            matches,
            selected_index: 0,
        }
    }
}

impl EventEmitter<OpenerConfirmed> for Picker<OpenerPickerDelegate> {}
impl EventEmitter<OpenerPickerDismissed> for Picker<OpenerPickerDelegate> {}

impl PickerDelegate for OpenerPickerDelegate {
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
        Arc::from("Choose an opener…")
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let candidates: Vec<StringMatchCandidate> = self
            .labels
            .iter()
            .enumerate()
            .map(|(ix, label)| StringMatchCandidate::new(ix, label))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel = AtomicBool::new(false);
        cx.spawn(async move |this, cx| {
            let matches = if query.trim().is_empty() {
                candidates
                    .iter()
                    .map(|c| fuzzy::StringMatch {
                        candidate_id: c.id,
                        score: 0.0,
                        positions: Vec::new(),
                        string: c.string.clone(),
                    })
                    .collect()
            } else {
                fuzzy::match_strings(
                    &candidates,
                    query.trim(),
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

    fn confirm(
        &mut self,
        _secondary: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(choice) = self.choices.get(matched.candidate_id).cloned() else {
            return;
        };
        cx.emit(OpenerConfirmed { choice });
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(OpenerPickerDismissed);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let matched = self.matches.get(ix)?;
        let choice = self.choices.get(matched.candidate_id)?;
        let icon = match choice {
            OpenerChoice::Default => IconName::Settings,
            OpenerChoice::Opener(_) => IconName::PlayOutlined,
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

/// Modal wrapper around the opener picker. Subscribes to the
/// delegate's emitted events and forwards them through a small
/// callback so the FM caller can stay in its own context (no need to
/// reach back into the picker entity).
pub(crate) struct OpenerPickerModal {
    picker: Entity<Picker<OpenerPickerDelegate>>,
    target_label: String,
    _subscriptions: Vec<Subscription>,
}

impl OpenerPickerModal {
    pub(crate) fn new<F>(
        choices: Vec<OpenerChoice>,
        target_label: String,
        on_confirm: F,
        _workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        F: Fn(OpenerChoice, &mut Window, &mut App) + 'static,
    {
        let delegate = OpenerPickerDelegate::new(choices);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx).modal(false));

        let on_confirm = Arc::new(on_confirm);
        let confirm = cx.subscribe_in(
            &picker,
            window,
            move |this, _, event: &OpenerConfirmed, window, cx| {
                let choice = event.choice.clone();
                let cb = on_confirm.clone();
                cx.defer_in(window, move |_, window, cx| {
                    cb(choice, window, cx);
                });
                this.dismiss(window, cx);
            },
        );
        let on_dismiss = cx.subscribe_in(
            &picker,
            window,
            |this, _, _: &OpenerPickerDismissed, window, cx| {
                this.dismiss(window, cx);
            },
        );

        Self {
            picker,
            target_label,
            _subscriptions: vec![confirm, on_dismiss],
        }
    }

    fn dismiss(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

impl ModalView for OpenerPickerModal {}
impl EventEmitter<DismissEvent> for OpenerPickerModal {}
impl Focusable for OpenerPickerModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for OpenerPickerModal {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.target_label.clone();
        div()
            .elevation_3(_cx)
            .w(rems(42.))
            .flex_1()
            .overflow_hidden()
            .child(
                v_flex()
                    .child(
                        h_flex().px_3().py_1().child(
                            Label::new(format!("open: {target}"))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                    )
                    .child(self.picker.clone()),
            )
    }
}

/// The targets a chosen opener should run over — either the single
/// cursor entry, or every entry in the marked set. The FM substitutes
/// `{path}` per target if the opener template needs per-entry expansion,
/// or once if the template uses the multi-path `{paths}` token.
#[derive(Clone, Debug)]
pub(crate) struct OpenerTargets {
    pub(crate) cursor: PathBuf,
    pub(crate) marked: Vec<PathBuf>,
    pub(crate) cwd: PathBuf,
}

impl OpenerTargets {
    /// `true` when the template references the multi-path tokens; in
    /// that case the opener runs once with `marked` joined, not once
    /// per marked entry. Cheap because templates are short.
    pub(crate) fn template_is_multi_path(template: &str) -> bool {
        template.contains("{paths}") || template.contains("{names}")
    }

    /// Iteration plan: when there's no marked set the only target is
    /// the cursor; when the template is multi-path-aware the marked set
    /// flows through `apply_substitutions` once; otherwise the opener
    /// runs once per marked entry.
    pub(crate) fn plan(&self, template: &str) -> Vec<(PathBuf, Vec<PathBuf>)> {
        if self.marked.is_empty() || Self::template_is_multi_path(template) {
            return vec![(self.cursor.clone(), self.marked.clone())];
        }
        self.marked
            .iter()
            .cloned()
            .map(|p| (p, Vec::new()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_path_template_runs_once_when_marks_empty() {
        let targets = OpenerTargets {
            cursor: PathBuf::from("/tmp/a.png"),
            marked: Vec::new(),
            cwd: PathBuf::from("/tmp"),
        };
        let plan = targets.plan("open {path}");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, PathBuf::from("/tmp/a.png"));
    }

    #[test]
    fn multi_path_template_runs_once_with_marks() {
        let targets = OpenerTargets {
            cursor: PathBuf::from("/tmp/a.png"),
            marked: vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
            cwd: PathBuf::from("/tmp"),
        };
        let plan = targets.plan("zip out.zip {paths}");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].1.len(), 2);
    }

    #[test]
    fn single_path_template_fans_out_across_marks() {
        let targets = OpenerTargets {
            cursor: PathBuf::from("/tmp/a.png"),
            marked: vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
            cwd: PathBuf::from("/tmp"),
        };
        let plan = targets.plan("qlmanage -p {path}");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].0, PathBuf::from("/tmp/a.png"));
        assert_eq!(plan[1].0, PathBuf::from("/tmp/b.png"));
    }

    #[test]
    fn names_token_is_treated_as_multi_path() {
        let targets = OpenerTargets {
            cursor: PathBuf::from("/tmp/a.png"),
            marked: vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
            cwd: PathBuf::from("/tmp"),
        };
        let plan = targets.plan("tar cf out.tar {names}");
        assert_eq!(plan.len(), 1);
    }
}
