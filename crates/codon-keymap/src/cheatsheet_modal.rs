//! Full-screen modal listing every keybinding currently reachable from the
//! workspace context. Bound to `cmd-k F1` by default.

use gpui::{
    Context, DismissEvent, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, KeyContext, ParentElement, Render, ScrollHandle, SharedString, Styled, Window,
    actions, div, px,
};
use ui::{
    ActiveTheme, Color, Divider, DividerColor, Headline, HeadlineSize, IconName, Label,
    LabelCommon, LabelSize, StatefulInteractiveElement, WithScrollbar, h_flex, text_for_keystrokes,
    v_flex,
};
use workspace::{ModalView, Workspace};

actions!(
    codon_keymap,
    [
        /// Show the keybindings cheatsheet — a full-screen list of every action
        /// bound in the current context.
        ShowKeymap
    ]
);

pub struct KeybindingsCheatsheetModal {
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    bindings: Vec<BindingRow>,
}

#[derive(Clone)]
struct BindingRow {
    keystrokes: SharedString,
    action_name: SharedString,
    namespace: SharedString,
}

impl KeybindingsCheatsheetModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        Self {
            focus_handle,
            scroll_handle: ScrollHandle::new(),
            bindings: collect_bindings(window, cx),
        }
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

fn collect_bindings(
    window: &mut Window,
    cx: &mut Context<KeybindingsCheatsheetModal>,
) -> Vec<BindingRow> {
    let raw = window.possible_bindings_for_input(&[]);
    let mut rows: Vec<BindingRow> = raw
        .iter()
        .filter_map(|binding| {
            let keystrokes: Vec<_> = binding
                .keystrokes()
                .iter()
                .map(|k| k.inner().to_owned())
                .collect();
            if keystrokes.is_empty() {
                return None;
            }
            let raw_name = binding.action().name();
            let humanized = command_palette::humanize_action_name(raw_name);
            let (namespace, _) = split_namespace(raw_name);
            Some(BindingRow {
                keystrokes: SharedString::from(text_for_keystrokes(&keystrokes, cx)),
                action_name: SharedString::from(humanized),
                namespace: SharedString::from(namespace),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        a.namespace
            .cmp(&b.namespace)
            .then_with(|| a.keystrokes.cmp(&b.keystrokes))
            .then_with(|| a.action_name.cmp(&b.action_name))
    });
    rows.dedup_by(|a, b| a.keystrokes == b.keystrokes && a.action_name == b.action_name);
    rows
}

fn split_namespace(raw_name: &str) -> (String, String) {
    if let Some((ns, _)) = raw_name.split_once("::") {
        (ns.replace('_', " "), raw_name.to_string())
    } else {
        (String::from("global"), raw_name.to_string())
    }
}

impl Render for KeybindingsCheatsheetModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let theme = cx.theme();
        let panel_bg = theme.colors().elevated_surface_background;
        let border = theme.colors().border;

        let mut grouped: Vec<(SharedString, Vec<BindingRow>)> = Vec::new();
        for row in &self.bindings {
            match grouped.last_mut() {
                Some((ns, items)) if ns == &row.namespace => items.push(row.clone()),
                _ => grouped.push((row.namespace.clone(), vec![row.clone()])),
            }
        }

        let mut key_context = KeyContext::default();
        key_context.add("KeybindingsCheatsheet");
        key_context.add("menu");

        let header = h_flex()
            .items_center()
            .justify_between()
            .pb_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(Headline::new("Keybindings").size(HeadlineSize::Medium))
                    .child(
                        Label::new(format!(
                            "{} bindings — Esc to close",
                            self.bindings.len()
                        ))
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                    ),
            )
            .child(ui::Icon::new(IconName::Command).color(Color::Muted));

        let body = if grouped.is_empty() {
            v_flex().py_8().child(
                Label::new("No keybindings registered yet.")
                    .color(Color::Muted)
                    .size(LabelSize::Default),
            )
        } else {
            let mut column = v_flex().gap_4();
            for (ns, items) in grouped {
                column = column.child(
                    v_flex()
                        .gap_1()
                        .child(
                            Label::new(ns.clone())
                                .color(Color::Muted)
                                .size(LabelSize::Small)
                                .weight(FontWeight::SEMIBOLD),
                        )
                        .child(Divider::horizontal().color(DividerColor::BorderFaded))
                        .children(items.into_iter().map(|row| {
                            h_flex()
                                .py_0p5()
                                .gap_4()
                                .child(div().min_w(px(180.)).child(
                                    Label::new(row.keystrokes.clone())
                                        .color(Color::Accent)
                                        .size(LabelSize::Default),
                                ))
                                .child(
                                    Label::new(row.action_name.clone())
                                        .color(Color::Default)
                                        .size(LabelSize::Default)
                                        .single_line()
                                        .truncate(),
                                )
                        })),
                );
            }
            column
        };

        let max_w = px((f32::from(viewport.width) * 0.85).min(960.));
        let max_h = px(f32::from(viewport.height) * 0.85);

        div()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::dismiss))
            .occlude()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.45))
            .child(
                v_flex()
                    .id("codon-keymap-cheatsheet")
                    .max_w(max_w)
                    .max_h(max_h)
                    .w_full()
                    .min_h(px(320.))
                    .rounded_lg()
                    .bg(panel_bg)
                    .border_1()
                    .border_color(border)
                    .px_6()
                    .py_5()
                    .child(header)
                    .child(
                        div()
                            .id("codon-keymap-rows")
                            .flex_grow()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .child(body)
                            .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                    ),
            )
    }
}

impl EventEmitter<DismissEvent> for KeybindingsCheatsheetModal {}

impl Focusable for KeybindingsCheatsheetModal {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for KeybindingsCheatsheetModal {
    fn render_bare(&self) -> bool {
        true
    }
}

pub fn show_keymap(
    workspace: &mut Workspace,
    _: &ShowKeymap,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    workspace.toggle_modal(window, cx, |window, cx| {
        KeybindingsCheatsheetModal::new(window, cx)
    });
}

pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(show_keymap);
}
