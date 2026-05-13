//! Full-screen modal listing every keybinding currently reachable from the
//! workspace context. Bound to `cmd-k F1` by default.

use std::rc::Rc;

use gpui::{
    AnyElement, Context, DismissEvent, ElementId, EventEmitter, FocusHandle, Focusable, FontWeight,
    Hsla, InteractiveElement, IntoElement, KeyBinding as GpuiKeyBinding, KeyContext,
    KeybindingKeystroke, ParentElement, Render, ScrollHandle, SharedString, Styled, Window,
    actions, div, prelude::FluentBuilder, px,
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
    /// Bindings whose predicate matches the *leaf* of the pane context
    /// stack captured before the modal opened — "this pane" verbs.
    local_bindings: Vec<BindingRow>,
    /// Everything else, including bindings with no predicate (apply
    /// everywhere) and bindings whose predicate matches at an outer
    /// level (e.g. `Workspace`, `Pane`).
    global_bindings: Vec<BindingRow>,
    /// Captured at open time so the empty-state hint can name the pane
    /// kind (e.g. "No GitStatus-specific bindings").
    leaf_context_label: Option<SharedString>,
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
    pub fn new(
        pane_context_stack: Vec<KeyContext>,
        raw_bindings: Vec<GpuiKeyBinding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let leaf_context_label = leaf_context_label(&pane_context_stack);
        let (local_bindings, global_bindings) =
            collect_bindings(&pane_context_stack, raw_bindings, cx);
        Self {
            focus_handle,
            scroll_handle: ScrollHandle::new(),
            local_bindings,
            global_bindings,
            leaf_context_label,
        }
    }

    fn dismiss(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

/// A flattened row used by the per-section `uniform_list`. Rows have a
/// constant rendered height so the list can virtualize — only items
/// intersecting the modal's visible scroll region are laid out and
/// painted.
#[derive(Clone)]
enum RowKind {
    /// Single muted line used in place of body rows when a section is empty
    /// (currently only the "This pane" section uses this).
    EmptyHint(SharedString),
    /// One paired entry. `left` is rendered in the left column, `right`
    /// (when present) in the right column. `pair_index_in_section` is the
    /// row's index among the section's `Pair` rows so striping stays put
    /// across scroll positions.
    Pair {
        left: BindingRow,
        right: Option<BindingRow>,
        pair_index_in_section: usize,
    },
}

/// Group a section's bindings into top-down-then-right pairs. The first
/// half of the list goes in the left column, the second half in the right —
/// the same visual order the old non-virtualized renderer produced.
fn build_pairs(items: &[BindingRow]) -> Vec<RowKind> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    let split = n.div_ceil(2);
    let mut rows = Vec::with_capacity(split);
    for pair_index in 0..split {
        let left = items[pair_index].clone();
        let right_index = pair_index + split;
        let right = items.get(right_index).cloned();
        rows.push(RowKind::Pair {
            left,
            right,
            pair_index_in_section: pair_index,
        });
    }
    rows
}

/// Render a single binding cell (chord + action name). Used twice per
/// `Pair` row.
fn render_binding_cell(row: &BindingRow) -> AnyElement {
    let chord = KeyBinding::from_keystrokes(row.keystrokes.clone(), false)
        .size(ui::rems_from_px(13.));
    h_flex()
        .items_center()
        .gap_3()
        .flex_1()
        .min_w(px(0.))
        .child(
            div()
                .min_w(px(140.))
                .flex_none()
                .child(h_flex().justify_end().child(chord)),
        )
        .child(
            Label::new(row.action_name.clone())
                .color(Color::Default)
                .size(LabelSize::Default)
                .single_line()
                .truncate(),
        )
        .into_any_element()
}

/// Render one `RowKind` as a constant-height element. Pair rows always
/// render two columns (with a placeholder spacer when the right slot is
/// empty) so every row in a section has the same height, which is what
/// `uniform_list` requires.
fn render_row(row: &RowKind, row_bg: Hsla) -> AnyElement {
    match row {
        RowKind::EmptyHint(text) => v_flex()
            .py_1()
            .child(
                Label::new(text.clone())
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            )
            .into_any_element(),
        RowKind::Pair {
            left,
            right,
            pair_index_in_section,
        } => {
            let striped = pair_index_in_section % 2 == 1;
            let left_cell = h_flex()
                .items_center()
                .px_2()
                .py_0p5()
                .rounded_md()
                .flex_1()
                .min_w(px(0.))
                .when(striped, |el| el.bg(row_bg))
                .child(render_binding_cell(left));
            let right_cell: AnyElement = match right {
                Some(binding) => h_flex()
                    .items_center()
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .flex_1()
                    .min_w(px(0.))
                    .when(striped, |el| el.bg(row_bg))
                    .child(render_binding_cell(binding))
                    .into_any_element(),
                // Keep the row height + column widths stable even when a
                // pair is missing its right entry (the last row in an
                // odd-count section).
                None => div().flex_1().min_w(px(0.)).into_any_element(),
            };
            h_flex()
                .gap_6()
                .items_start()
                .child(left_cell)
                .child(right_cell)
                .into_any_element()
        }
    }
}

/// Render a complete section: accent-bar header, divider, then the body
/// rows. Rows are flattened into `RowKind` (header-less; the section header
/// is its own sibling) so striping anchors on `pair_index_in_section` and
/// stays put across scroll positions.
fn render_section(
    _id: impl Into<ElementId>,
    label: SharedString,
    count: usize,
    rows: Vec<RowKind>,
    accent: Hsla,
    divider: Hsla,
    row_bg: Hsla,
) -> AnyElement {
    let header_row = h_flex()
        .items_center()
        .gap_2()
        .pb_1()
        .child(div().w(px(3.)).h(px(14.)).rounded_full().bg(accent))
        .child(
            Label::new(label)
                .color(Color::Default)
                .size(LabelSize::Default)
                .weight(FontWeight::SEMIBOLD),
        )
        .child(
            Label::new(format!("{count}"))
                .color(Color::Muted)
                .size(LabelSize::Small),
        );

    let body = v_flex()
        .w_full()
        .children(rows.iter().map(|row| render_row(row, row_bg)));

    v_flex()
        .gap_1()
        .child(header_row)
        .child(div().h(px(1.)).w_full().bg(divider))
        .child(body)
        .into_any_element()
}

/// Best-effort short name for the deepest `KeyContext` on the stack —
/// used as a label for the "This pane" section. Falls back to a generic
/// label when the leaf has no primary identifier.
fn leaf_context_label(stack: &[KeyContext]) -> Option<SharedString> {
    let leaf = stack.last()?;
    leaf.primary().map(|entry| entry.key.clone())
}

fn collect_bindings(
    pane_context_stack: &[KeyContext],
    raw: Vec<GpuiKeyBinding>,
    cx: &mut Context<KeybindingsCheatsheetModal>,
) -> (Vec<BindingRow>, Vec<BindingRow>) {
    // `raw` is captured pre-modal in `show_keymap` so it includes
    // pane-specific bindings (`Terminal && pane_mode == normal`,
    // `GitStatus`, etc.) that the modal's own focus context would
    // otherwise filter out by the time `possible_bindings_for_input`
    // ran inside `new`. Ordered by precedence: deeper context first, then
    // more-recently-registered first. The user's codon.toml is loaded
    // *after* the embedded defaults, so a user override appears before
    // the corresponding default in `raw`. Collapse all bindings that
    // share a (chord, context) pair down to the first occurrence so the
    // cheatsheet shows what would actually fire — never both.
    let mut local: Vec<BindingRow> = Vec::new();
    let mut global: Vec<BindingRow> = Vec::with_capacity(raw.len());
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
        let row = BindingRow {
            keystrokes: Rc::from(keystrokes),
            keystrokes_text,
            action_name: SharedString::from(humanized),
            namespace: SharedString::from(namespace),
        };
        // A binding is *pane-local* iff its predicate is satisfied by the
        // full stack but NOT by the stack with the leaf removed — i.e.
        // the leaf context is load-bearing. Plain "matches at leaf depth"
        // isn't enough because predicates like `!Editor`,
        // `Pane && something`, or anything that's true at multiple levels
        // also satisfies `depth_of == stack.len()` and would flood the
        // "this pane" section with broadly-applicable bindings.
        let is_local = match binding.predicate() {
            Some(predicate) => {
                if pane_context_stack.is_empty() {
                    false
                } else {
                    let matches_full = predicate.depth_of(pane_context_stack).is_some();
                    let matches_without_leaf = pane_context_stack.len() > 1
                        && predicate
                            .depth_of(&pane_context_stack[..pane_context_stack.len() - 1])
                            .is_some();
                    matches_full && !matches_without_leaf
                }
            }
            None => false,
        };
        if is_local {
            local.push(row);
        } else {
            global.push(row);
        }
    }
    let local_sort = |a: &BindingRow, b: &BindingRow| {
        chord_sort_key(&a.keystrokes_text)
            .cmp(&chord_sort_key(&b.keystrokes_text))
            .then_with(|| a.action_name.cmp(&b.action_name))
    };
    local.sort_by(local_sort);
    global.sort_by(|a, b| {
        namespace_priority(&a.namespace)
            .cmp(&namespace_priority(&b.namespace))
            .then_with(|| a.namespace.cmp(&b.namespace))
            .then_with(|| chord_sort_key(&a.keystrokes_text).cmp(&chord_sort_key(&b.keystrokes_text)))
            .then_with(|| a.action_name.cmp(&b.action_name))
    });
    (local, global)
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
        for row in &self.global_bindings {
            match grouped.last_mut() {
                Some((ns, items)) if ns == &row.namespace => items.push(row.clone()),
                _ => grouped.push((row.namespace.clone(), vec![row.clone()])),
            }
        }
        let total_count = self.local_bindings.len() + self.global_bindings.len();

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
                            total_count
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

        let body = if grouped.is_empty() && self.local_bindings.is_empty() {
            v_flex().py_8().child(
                Label::new("No keybindings registered yet.")
                    .color(Color::Muted)
                    .size(LabelSize::Default),
            )
        } else {
            let accent = theme.colors().text_accent;
            let mut column = v_flex().gap_5();

            // "This pane" section — always rendered (with a muted hint
            // when empty) so the layout doesn't shift between panes.
            {
                let label = self
                    .leaf_context_label
                    .clone()
                    .map(|leaf| SharedString::from(format!("This pane · {leaf}")))
                    .unwrap_or_else(|| SharedString::from("This pane"));
                let count = self.local_bindings.len();
                let rows: Vec<RowKind> = if self.local_bindings.is_empty() {
                    vec![RowKind::EmptyHint(SharedString::from(
                        "No pane-specific bindings",
                    ))]
                } else {
                    build_pairs(&self.local_bindings)
                };
                column = column.child(render_section(
                    ElementId::from("cheatsheet-section-this-pane"),
                    label,
                    count,
                    rows,
                    accent,
                    border_faded,
                    row_bg,
                ));
            }

            for (idx, (ns, items)) in grouped.into_iter().enumerate() {
                let count = items.len();
                let rows = build_pairs(&items);
                column = column.child(render_section(
                    ElementId::from(("cheatsheet-section", idx)),
                    ns,
                    count,
                    rows,
                    accent,
                    border_faded,
                    row_bg,
                ));
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
    // Capture the focus chain *and* the reachable bindings before the
    // modal opens — `toggle_modal` shifts focus to the cheatsheet, and
    // `possible_bindings_for_input` filters by the *current* dispatch
    // stack, so anything pane-specific (`Terminal && pane_mode ==
    // normal`, `GitStatus`, …) would be missing if we called it inside
    // `new`.
    let pane_context_stack = window.context_stack();
    let raw_bindings = window.possible_bindings_for_input(&[]);
    workspace.toggle_modal(window, cx, move |window, cx| {
        KeybindingsCheatsheetModal::new(pane_context_stack, raw_bindings, window, cx)
    });
}

pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(show_keymap);
}
