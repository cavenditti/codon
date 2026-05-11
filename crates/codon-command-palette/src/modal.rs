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

use std::sync::Arc;

use command_palette::{humanize_action_name, normalize_action_query};
use command_palette_hooks::CommandPaletteFilter;
use fuzzy::{StringMatch, StringMatchCandidate};
use gpui::{
    Action, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, Render, Task, WeakEntity, Window,
};
use picker::{Picker, PickerDelegate};
use ui::{
    HighlightedLabel, Icon, IconName, IconSize, KeyBinding, Label, LabelCommon as _, LabelSize,
    ListItem, ListItemSpacing, SharedString, prelude::*,
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
        // Mirror picker re-renders into the outer modal so the side
        // description panel updates the moment the picker's selection or
        // matches change. Without this, the description stays frozen on
        // the first matched row.
        cx.observe(&picker, |_, _, cx| cx.notify()).detach();
        Self { picker }
    }

    /// Snapshot the data needed by the side description panel from the
    /// picker's delegate. Read-only — runs on every render of `CodonPalette`.
    fn aside_snapshot(&self, cx: &App) -> Option<AsideSnapshot> {
        let picker = self.picker.read(cx);
        let d = &picker.delegate;
        match &d.mode {
            Mode::Command => {
                let m = d.matches.get(d.selected_ix)?;
                let cmd = d.all_commands.get(m.candidate_id)?;
                let arg_hint = completer::for_action_name(cmd.action.name()).map(|c| {
                    SharedString::from(format!(
                        "Type `{} ` to set arguments",
                        c.aliases()[0]
                    ))
                });
                Some(AsideSnapshot {
                    title: cmd.name.clone(),
                    chord_action: Some(cmd.action.clone()),
                    focus: d.previous_focus_handle.clone(),
                    doc: cmd.documentation.clone(),
                    arg_hint,
                    enter_preview: Some(SharedString::from(format!("↵ run {}", cmd.name))),
                })
            }
            Mode::Argument {
                completer,
                command_label,
                command_doc,
            } => {
                let item = d.arg_items.get(d.selected_ix);
                let enter_preview = item.map(|item| match &item.navigates_to {
                    Some(nav) => SharedString::from(format!(
                        "↵ open {} {} (navigate)",
                        completer.aliases()[0],
                        nav
                    )),
                    None => {
                        let verb = completer.aliases()[0];
                        SharedString::from(format!("↵ {verb} {}", item.label))
                    }
                });
                Some(AsideSnapshot {
                    title: command_label.clone(),
                    chord_action: None,
                    focus: d.previous_focus_handle.clone(),
                    doc: command_doc.clone(),
                    arg_hint: Some(SharedString::from(format!(
                        "Argument: {}",
                        completer.placeholder()
                    ))),
                    enter_preview,
                })
            }
        }
    }
}

struct AsideSnapshot {
    title: SharedString,
    chord_action: Option<Arc<dyn Action>>,
    focus: FocusHandle,
    doc: Option<SharedString>,
    arg_hint: Option<SharedString>,
    /// One-line "what Enter will do" preview — appended to the aside so a
    /// keyboard-only user always sees what's about to fire. (A true in-input
    /// ghost-text overlay would need editor-level surgery; this is the
    /// pragmatic substitute.)
    enter_preview: Option<SharedString>,
}

fn render_aside(snap: AsideSnapshot, cx: &App) -> AnyElement {
    let mut col = v_flex()
        .w(rems(20.))
        .p_3()
        .gap_2()
        .child(Label::new(snap.title).weight(FontWeight::BOLD));
    if let Some(action) = snap.chord_action.as_ref() {
        col = col.child(KeyBinding::for_action_in(action.as_ref(), &snap.focus, cx));
    }
    if let Some(d) = snap.doc {
        col = col.child(Label::new(d).size(LabelSize::Small));
    }
    if let Some(h) = snap.arg_hint {
        col = col.child(
            Label::new(h)
                .size(LabelSize::Small)
                .color(Color::Muted),
        );
    }
    if let Some(p) = snap.enter_preview {
        col = col.child(
            Label::new(p)
                .size(LabelSize::Small)
                .color(Color::Accent),
        );
    }
    col.into_any_element()
}

impl EventEmitter<DismissEvent> for CodonPalette {}

impl Focusable for CodonPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl ModalView for CodonPalette {}

impl Render for CodonPalette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let aside = self.aside_snapshot(cx);
        // The modal layer centers our rendered tree horizontally. We want
        // the *picker* centered (the user sees it where every other modal
        // lives), with the description panel hanging off to the right —
        // not contributing to the centered footprint. So: a 34rem-wide
        // outer container holds the picker; the panel is an absolutely-
        // positioned sibling anchored to that container's right edge.
        div()
            .key_context("CodonCommandPalette")
            .relative()
            .w(rems(34.))
            .child(self.picker.clone())
            .when_some(aside, |this, snap| {
                this.child(
                    div()
                        .absolute()
                        .left_full()
                        .ml_2()
                        .top_0()
                        .elevation_2(cx)
                        .child(render_aside(snap, cx)),
                )
            })
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
                // Navigable items (directories under the file-path
                // completer) fill the input and re-run the completer
                // instead of dispatching — stay in the palette so the
                // user can drill in further.
                if let Some(nav) = &item.navigates_to {
                    let new_query = format!("{} {}", completer.aliases()[0], nav);
                    cx.defer_in(window, move |picker, window, cx| {
                        picker.set_query(&new_query, window, cx);
                    });
                    return;
                }
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
                // Demote to Command mode instead of closing. Restore the
                // query to just the verb — *no* trailing space, otherwise
                // update_matches would immediately re-trigger Argument
                // mode and Esc would appear to do nothing. Another Esc
                // from Command mode closes the modal.
                let restore = completer.aliases()[0].to_string();
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
                let is_nav = item.navigates_to.is_some();
                // Single-row layout: leading folder/file icon, then label
                // tinted by kind. Drops the previous second-line detail so
                // every entry is the same height — file-manager-style.
                let icon = if is_nav { IconName::Folder } else { IconName::File };
                let icon_color = if is_nav { Color::Accent } else { Color::Muted };
                let label_color = if is_nav { Color::Accent } else { Color::Default };
                Some(
                    ListItem::new(ix)
                        .inset(true)
                        .spacing(ListItemSpacing::Sparse)
                        .toggle_state(selected)
                        .child(
                            h_flex()
                                .gap_2()
                                .py_px()
                                .child(Icon::new(icon).color(icon_color).size(IconSize::Small))
                                .child(Label::new(label).color(label_color)),
                        ),
                )
            }
        }
    }

    // Description side panel is rendered by the outer `CodonPalette` —
    // see `CodonPalette::render` + `aside_snapshot`. We deliberately do
    // not implement `documentation_aside` here: the picker's built-in
    // aside bootstraps from a paint-time canvas, so it never shows on
    // first render. Rendering the panel from the outer modal updates
    // every time the picker re-renders.
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
