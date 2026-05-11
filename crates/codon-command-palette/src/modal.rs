//! Codon command-palette modal + picker delegate.
//!
//! Single picker delegate, internal `Mode` state machine:
//!
//! ```text
//!  Command  ── alias + space typed ──▶  Argument
//!                          ◀── Esc / verb erased ──
//! ```
//!
//! In `Command` mode the row list is every registered Zed action filtered
//! by fuzzy match against the humanized name. In `Argument` mode the row
//! list is whatever the registered [`Completer`](crate::completer::Completer)
//! returns for the current argument query.
//!
//! The description aside (`PickerDelegate::documentation_aside`) is always
//! populated so a keyboard user sees what the selected command does without
//! hovering — this is the always-visible description pane required by
//! `REQ:codon/command-palette#c-description-pane`.

use std::{rc::Rc, sync::Arc};

use command_palette::{humanize_action_name, normalize_action_query};
use command_palette_hooks::CommandPaletteFilter;
use fuzzy::{StringMatch, StringMatchCandidate};
use gpui::{
    Action, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, Render, Task, WeakEntity, Window,
};
use picker::{Picker, PickerDelegate};
use ui::{
    DocumentationAside, DocumentationSide, HighlightedLabel, KeyBinding, Label, LabelCommon as _,
    LabelSize, ListItem, ListItemSpacing, SharedString, prelude::*,
};
use util::ResultExt as _;
use workspace::{ModalView, Workspace};

use crate::completer::{self, CompletionItem, Completer};

pub struct CodonPalette {
    picker: Entity<Picker<CodonPaletteDelegate>>,
}

impl CodonPalette {
    pub fn toggle(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        let Some(previous_focus_handle) = window.focused(cx) else {
            return;
        };
        let workspace_handle = cx.weak_entity();
        workspace.toggle_modal(window, cx, move |window, cx| {
            CodonPalette::new(previous_focus_handle, workspace_handle, window, cx)
        });
    }

    fn new(
        previous_focus_handle: FocusHandle,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.entity().downgrade();
        let commands = collect_commands(window, cx);

        let delegate = CodonPaletteDelegate {
            palette,
            workspace,
            previous_focus_handle,
            all_commands: commands,
            matches: Vec::new(),
            mode: Mode::Command,
            selected_ix: 0,
            arg_items: Vec::new(),
        };

        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx));
        Self { picker }
    }
}

impl EventEmitter<DismissEvent> for CodonPalette {}

impl Focusable for CodonPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl ModalView for CodonPalette {}

impl Render for CodonPalette {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("CodonCommandPalette")
            .w(rems(34.))
            .child(self.picker.clone())
    }
}

#[derive(Clone)]
struct Command {
    name: SharedString,
    action: Arc<dyn Action>,
    documentation: Option<SharedString>,
}

#[derive(Clone)]
enum Mode {
    Command,
    Argument {
        completer: Arc<dyn Completer>,
        command_label: SharedString,
        command_doc: Option<SharedString>,
    },
}

pub struct CodonPaletteDelegate {
    palette: WeakEntity<CodonPalette>,
    workspace: WeakEntity<Workspace>,
    previous_focus_handle: FocusHandle,
    all_commands: Vec<Command>,
    matches: Vec<StringMatch>,
    mode: Mode,
    selected_ix: usize,
    arg_items: Vec<CompletionItem>,
}

fn collect_commands(window: &mut Window, cx: &mut App) -> Vec<Command> {
    let filter = CommandPaletteFilter::try_global(cx);
    let docs = cx.action_documentation().clone();
    window
        .available_actions(cx)
        .into_iter()
        .filter_map(|action| {
            if filter.is_some_and(|f| f.is_hidden(&*action)) {
                return None;
            }
            let name: SharedString = humanize_action_name(action.name()).into();
            let documentation = docs.get(action.name()).map(|s| SharedString::from(*s));
            Some(Command {
                name,
                action: Arc::from(action),
                documentation,
            })
        })
        .collect()
}

impl PickerDelegate for CodonPaletteDelegate {
    type ListItem = ListItem;

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        match &self.mode {
            Mode::Command => "Execute a command (verb + space → arguments)…".into(),
            Mode::Argument { completer, .. } => completer.placeholder().into(),
        }
    }

    fn match_count(&self) -> usize {
        match &self.mode {
            Mode::Command => self.matches.len(),
            Mode::Argument { .. } => self.arg_items.len(),
        }
    }

    fn selected_index(&self) -> usize {
        self.selected_ix
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_ix = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let trigger = parse_trigger(&query)
            .and_then(|(alias, rest)| completer::for_alias(alias).map(|c| (c, rest)));

        match (&self.mode, trigger) {
            (Mode::Command, None) => self.spawn_command_match(query, cx),
            (Mode::Command, Some((completer, rest))) => {
                self.enter_argument_mode(completer, rest, cx)
            }
            (Mode::Argument { completer, .. }, _) => {
                let completer = completer.clone();
                match strip_alias_prefix(&query, completer.aliases()) {
                    Some(arg_query) => self.spawn_argument_match(completer, arg_query, cx),
                    None => {
                        self.mode = Mode::Command;
                        self.selected_ix = 0;
                        self.arg_items.clear();
                        self.spawn_command_match(query, cx)
                    }
                }
            }
        }
    }

    fn confirm(
        &mut self,
        _secondary: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let action: Box<dyn Action> = match &self.mode {
            Mode::Command => {
                let Some(m) = self.matches.get(self.selected_ix) else {
                    self.emit_dismiss(cx);
                    return;
                };
                let Some(cmd) = self.all_commands.get(m.candidate_id) else {
                    self.emit_dismiss(cx);
                    return;
                };
                cmd.action.boxed_clone()
            }
            Mode::Argument { completer, .. } => {
                let Some(item) = self.arg_items.get(self.selected_ix) else {
                    self.emit_dismiss(cx);
                    return;
                };
                completer.build_action(&item.value)
            }
        };
        window.focus(&self.previous_focus_handle, cx);
        self.emit_dismiss(cx);
        window.dispatch_action(action, cx);
    }

    fn dismissed(&mut self, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        match &self.mode {
            Mode::Argument { command_label, completer, .. } => {
                // Demote to Command mode instead of closing. Repopulate the
                // query with the verb so the user knows where they came
                // from; another Esc from Command mode closes.
                let restore = format!("{} ", completer.aliases()[0]);
                let _ = command_label; // kept for future use / aside
                self.mode = Mode::Command;
                self.selected_ix = 0;
                self.arg_items.clear();
                cx.defer_in(window, move |picker, window, cx| {
                    picker.set_query(&restore, window, cx);
                });
            }
            Mode::Command => self.emit_dismiss(cx),
        }
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        match &self.mode {
            Mode::Command => {
                let m = self.matches.get(ix)?;
                let cmd = self.all_commands.get(m.candidate_id)?;
                Some(
                    ListItem::new(ix)
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .toggle_state(selected)
                        .child(
                            h_flex()
                                .w_full()
                                .py_px()
                                .justify_between()
                                .child(HighlightedLabel::new(
                                    cmd.name.clone(),
                                    m.positions.clone(),
                                ))
                                .child(KeyBinding::for_action_in(
                                    cmd.action.as_ref(),
                                    &self.previous_focus_handle,
                                    cx,
                                )),
                        ),
                )
            }
            Mode::Argument { .. } => {
                let item = self.arg_items.get(ix)?;
                let label = item.label.clone();
                let detail = item.detail.clone();
                Some(
                    ListItem::new(ix)
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .toggle_state(selected)
                        .child(
                            v_flex()
                                .py_px()
                                .child(Label::new(label))
                                .when_some(detail, |this, d| {
                                    this.child(
                                        Label::new(d).size(LabelSize::Small).color(Color::Muted),
                                    )
                                }),
                        ),
                )
            }
        }
    }

    fn documentation_aside(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<DocumentationAside> {
        let (title, chord_action, doc, arg_hint) = match &self.mode {
            Mode::Command => {
                let m = self.matches.get(self.selected_ix)?;
                let cmd = self.all_commands.get(m.candidate_id)?;
                let arg_hint = completer::for_action_name(cmd.action.name()).map(|c| {
                    SharedString::from(format!(
                        "Type `{} ` to set arguments",
                        c.aliases()[0]
                    ))
                });
                (
                    cmd.name.clone(),
                    Some(cmd.action.clone()),
                    cmd.documentation.clone(),
                    arg_hint,
                )
            }
            Mode::Argument {
                completer,
                command_label,
                command_doc,
            } => (
                command_label.clone(),
                None,
                command_doc.clone(),
                Some(SharedString::from(format!(
                    "Argument: {}",
                    completer.placeholder()
                ))),
            ),
        };
        let focus = self.previous_focus_handle.clone();
        let render: Rc<dyn Fn(&mut App) -> AnyElement> = Rc::new(move |cx: &mut App| {
            let mut col = v_flex()
                .gap_1()
                .child(Label::new(title.clone()).weight(FontWeight::BOLD));
            if let Some(action) = chord_action.as_ref() {
                col = col.child(KeyBinding::for_action_in(action.as_ref(), &focus, cx));
            }
            if let Some(d) = doc.as_ref() {
                col = col.child(Label::new(d.clone()).size(LabelSize::Small));
            }
            if let Some(h) = arg_hint.as_ref() {
                col = col.child(
                    Label::new(h.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                );
            }
            col.into_any_element()
        });
        Some(DocumentationAside::new(DocumentationSide::Right, render))
    }

    fn documentation_aside_index(&self) -> Option<usize> {
        Some(self.selected_ix)
    }
}

impl CodonPaletteDelegate {
    fn emit_dismiss(&self, cx: &mut Context<Picker<Self>>) {
        self.palette
            .update(cx, |_, cx| cx.emit(DismissEvent))
            .ok();
    }

    fn spawn_command_match(
        &mut self,
        query: String,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let candidates: Vec<StringMatchCandidate> = self
            .all_commands
            .iter()
            .enumerate()
            .map(|(ix, c)| StringMatchCandidate::new(ix, &c.name))
            .collect();
        let executor = cx.background_executor().clone();
        let normalized = normalize_action_query(&query);
        cx.spawn(async move |picker, cx| {
            let matches = fuzzy::match_strings(
                &candidates,
                &normalized,
                false,
                true,
                10_000,
                &Default::default(),
                executor,
            )
            .await;
            picker
                .update(cx, |picker, cx| {
                    picker.delegate.matches = matches;
                    picker.delegate.selected_ix = 0;
                    cx.notify();
                })
                .ok();
        })
    }

    fn spawn_argument_match(
        &mut self,
        completer: Arc<dyn Completer>,
        arg_query: String,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let workspace = self.workspace.clone();
        let task = completer.complete(&arg_query, workspace, cx);
        cx.spawn(async move |picker, cx| {
            let items = task.await.log_err().unwrap_or_default();
            picker
                .update(cx, |picker, cx| {
                    picker.delegate.arg_items = items;
                    picker.delegate.selected_ix = 0;
                    cx.notify();
                })
                .ok();
        })
    }

    fn enter_argument_mode(
        &mut self,
        completer: Arc<dyn Completer>,
        arg_query: String,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let target = completer.action_name();
        let (command_label, command_doc) = self
            .all_commands
            .iter()
            .find(|c| c.action.name() == target)
            .map(|c| (c.name.clone(), c.documentation.clone()))
            .unwrap_or_else(|| (SharedString::from(target.to_string()), None));
        self.mode = Mode::Argument {
            completer: completer.clone(),
            command_label,
            command_doc,
        };
        self.selected_ix = 0;
        self.spawn_argument_match(completer, arg_query, cx)
    }
}

fn parse_trigger(query: &str) -> Option<(&str, String)> {
    let trimmed = query.trim_start();
    let space_ix = trimmed.find(' ')?;
    let alias = &trimmed[..space_ix];
    if alias.is_empty() {
        return None;
    }
    let rest = trimmed[space_ix + 1..].to_string();
    Some((alias, rest))
}

fn strip_alias_prefix(query: &str, aliases: &[&'static str]) -> Option<String> {
    let trimmed = query.trim_start();
    for alias in aliases {
        if let Some(rest) = trimmed.strip_prefix(alias)
            && let Some(after_space) = rest.strip_prefix(' ')
        {
            return Some(after_space.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trigger_handles_space_and_rest() {
        assert_eq!(parse_trigger("open"), None);
        assert_eq!(parse_trigger("open "), Some(("open", "".into())));
        assert_eq!(parse_trigger("open foo"), Some(("open", "foo".into())));
        assert_eq!(
            parse_trigger("  theme dark"),
            Some(("theme", "dark".into()))
        );
        assert_eq!(parse_trigger(""), None);
    }

    #[test]
    fn strip_alias_prefix_handles_aliases() {
        let aliases: &[&'static str] = &["open", "e"];
        assert_eq!(strip_alias_prefix("open foo", aliases).as_deref(), Some("foo"));
        assert_eq!(strip_alias_prefix("e bar", aliases).as_deref(), Some("bar"));
        assert_eq!(strip_alias_prefix("openfoo", aliases), None);
        assert_eq!(strip_alias_prefix("other", aliases), None);
    }
}
