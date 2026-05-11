//! Full-screen modal listing every keybinding currently reachable from the
//! workspace context. Bound to `cmd-k F1` by default.

use std::rc::Rc;

use gpui::{
    Context, DismissEvent, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, KeyContext, KeybindingKeystroke, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Window, actions, div, prelude::FluentBuilder, px,
};
use ui::{
    ActiveTheme, Color, Headline, HeadlineSize, IconName, KeyBinding, Label, LabelCommon,
    LabelSize, StatefulInteractiveElement, WithScrollbar, h_flex, text_for_keystrokes, v_flex,
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
    keystrokes: Rc<[KeybindingKeystroke]>,
    /// Pre-rendered text used for sorting and dedup so visually-equivalent
    /// rows collapse into one.
    keystrokes_text: SharedString,
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
    // `raw` arrives ordered by precedence: deeper context first, then
    // more-recently-registered first. The user's codon.toml is loaded
    // *after* the embedded defaults, so a user override appears before
    // the corresponding default in `raw`. Collapse all bindings that
    // share a (chord, context) pair down to the first occurrence so the
    // cheatsheet shows what would actually fire — never both.
    let mut rows: Vec<BindingRow> = Vec::with_capacity(raw.len());
    let mut seen: std::collections::HashSet<(SharedString, String)> =
        std::collections::HashSet::with_capacity(raw.len());
    for binding in raw.iter() {
        let keystrokes = binding.keystrokes();
        if keystrokes.is_empty() {
            continue;
        }
        let raw_keystrokes: Vec<_> = keystrokes.iter().map(|k| k.inner().to_owned()).collect();
        let keystrokes_text: SharedString = text_for_keystrokes(&raw_keystrokes, cx).into();
        let context_key = binding
            .predicate()
            .map(|p| format!("{p}"))
            .unwrap_or_default();
        if !seen.insert((keystrokes_text.clone(), context_key)) {
            continue;
        }
        let raw_name = binding.action().name();
        let humanized = command_palette::humanize_action_name(raw_name);
        let namespace = humanize_namespace(raw_name);
        rows.push(BindingRow {
            keystrokes: Rc::from(keystrokes),
            keystrokes_text,
            action_name: SharedString::from(humanized),
            namespace: SharedString::from(namespace),
        });
    }
    rows.sort_by(|a, b| {
        namespace_priority(&a.namespace)
            .cmp(&namespace_priority(&b.namespace))
            .then_with(|| a.namespace.cmp(&b.namespace))
            .then_with(|| chord_sort_key(&a.keystrokes_text).cmp(&chord_sort_key(&b.keystrokes_text)))
            .then_with(|| a.action_name.cmp(&b.action_name))
    });
    rows
}

fn humanize_namespace(raw_name: &str) -> String {
    let ns = raw_name.split_once("::").map(|(ns, _)| ns).unwrap_or("global");
    let pretty = ns.replace('_', " ");
    let mut chars = pretty.chars();
    chars
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or(pretty)
}

/// Codon-defined namespaces float to the top.
fn namespace_priority(namespace: &SharedString) -> u8 {
    let lower = namespace.to_ascii_lowercase();
    if lower.starts_with("codon") {
        0
    } else if lower == "global" {
        1
    } else {
        2
    }
}

/// Sort by chord length (shorter first), then by text. So `cmd-k a` sorts
/// before `cmd-k a a`, and bindings without a chord prefix come first within
/// a section.
fn chord_sort_key(text: &str) -> (usize, String) {
    (text.split_whitespace().count(), text.to_string())
}

impl Render for KeybindingsCheatsheetModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let theme = cx.theme();
        let panel_bg = theme.colors().elevated_surface_background;
        let row_bg = theme.colors().surface_background;
        let border = theme.colors().border;
        let border_faded = theme.colors().border_variant;

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
                    .gap_0p5()
                    .child(Headline::new("Keybindings").size(HeadlineSize::Medium))
                    .child(
                        Label::new(format!(
                            "{} bindings · Esc to dismiss",
                            self.bindings.len()
                        ))
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Label::new("⌘ K  F1").color(Color::Muted).size(LabelSize::Small))
                    .child(ui::Icon::new(IconName::Command).color(Color::Muted)),
            );

        let body = if grouped.is_empty() {
            v_flex().py_8().child(
                Label::new("No keybindings registered yet.")
                    .color(Color::Muted)
                    .size(LabelSize::Default),
            )
        } else {
            let mut column = v_flex().gap_5();
            for (ns, items) in grouped {
                let count = items.len();
                let header_row = h_flex()
                    .items_center()
                    .gap_2()
                    .pb_1()
                    .child(
                        div()
                            .w(px(3.))
                            .h(px(14.))
                            .rounded_full()
                            .bg(theme.colors().text_accent),
                    )
                    .child(
                        Label::new(ns.clone())
                            .color(Color::Default)
                            .size(LabelSize::Default)
                            .weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        Label::new(format!("{count}"))
                            .color(Color::Muted)
                            .size(LabelSize::Small),
                    );

                let mut rows_column = v_flex().gap_0p5();
                for (idx, row) in items.into_iter().enumerate() {
                    let chord = KeyBinding::from_keystrokes(row.keystrokes.clone(), false)
                        .size(ui::rems_from_px(13.));
                    let row_el = h_flex()
                        .items_center()
                        .gap_4()
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .when(idx % 2 == 1, |el| el.bg(row_bg))
                        .child(
                            div()
                                .min_w(px(180.))
                                .flex_none()
                                .child(h_flex().justify_end().child(chord)),
                        )
                        .child(
                            Label::new(row.action_name.clone())
                                .color(Color::Default)
                                .size(LabelSize::Default)
                                .single_line()
                                .truncate(),
                        );
                    rows_column = rows_column.child(row_el);
                }

                column = column.child(
                    v_flex()
                        .gap_1()
                        .child(header_row)
                        .child(
                            div()
                                .h(px(1.))
                                .w_full()
                                .bg(border_faded),
                        )
                        .child(rows_column),
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
            .bg(gpui::black().opacity(0.55))
            .child(
                v_flex()
                    .id("codon-keymap-cheatsheet")
                    .max_w(max_w)
                    .max_h(max_h)
                    .w_full()
                    .min_h(px(360.))
                    .rounded_lg()
                    .bg(panel_bg)
                    .border_1()
                    .border_color(border)
                    .shadow_lg()
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
