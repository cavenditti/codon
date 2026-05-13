//! Full-screen modal listing every keybinding currently reachable from the
//! workspace context. Bound to `cmd-k F1` by default.
//!
//! Only bindings declared in codon's curated set are shown — that's
//! everything in the embedded `DEFAULT_KEYMAP` plus everything the user
//! added to `~/.config/codon/codon.toml`. Vendor/zed's ~1000+ upstream
//! defaults are filtered out: they're noise to a codon user.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    AnyElement, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyBinding as GpuiKeyBinding, KeyContext, KeyDownEvent,
    KeybindingKeystroke, ListAlignment, ListState, ParentElement, Render, SharedString, Styled,
    Window, actions, div, list, prelude::FluentBuilder, px,
};
use ui::{
    ActiveTheme, Color, Headline, HeadlineSize, IconName, KeyBinding, Label, LabelCommon,
    LabelSize, h_flex, text_for_keystrokes, v_flex,
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
    /// All rows flattened into one ordered list: section headers
    /// interleaved with their pair rows. `gpui::list` virtualizes against
    /// this slice — only rows intersecting the modal's visible region get
    /// laid out and painted, which keeps the layout tree small enough that
    /// scrolling and Esc feel instant.
    rows: Rc<[RowKind]>,
    list_state: ListState,
    /// Cursor index into `rows`. `j` / `k` bump it; the cursored row gets
    /// a subtle highlight so the user can see where they are.
    cursor: usize,
    /// Set to true on dismiss so any in-flight paint frames during the
    /// modal fade-out render an empty body — defends against the modal
    /// layer continuing to call `render` while it animates away.
    dismissed: bool,
}

#[derive(Clone)]
struct BindingRow {
    keystrokes: Rc<[KeybindingKeystroke]>,
    keystrokes_text: SharedString,
    /// Humanized form (e.g. "Codon Session: Session Overview") for display.
    action_name: SharedString,
    /// Type-path form (e.g. "codon_session::SessionOverview") used for
    /// `cx.build_action` dispatch when the user presses Enter.
    raw_action_name: SharedString,
}

#[derive(Clone)]
enum RowKind {
    /// Group header — bold label + count badge.
    Header { label: SharedString, count: usize },
    /// One-line muted hint, used in place of pair rows when "This pane"
    /// has no bindings.
    EmptyHint(SharedString),
    /// One pair of side-by-side bindings. `right` is `None` for the last
    /// row in an odd-count section; the renderer fills the missing column
    /// with a spacer to keep column widths stable.
    Pair {
        left: BindingRow,
        right: Option<BindingRow>,
        striped: bool,
    },
}

/// Visual row heights, used only to choose a reasonable list overdraw and
/// to compute page-size jumps for ctrl-d / ctrl-u. The list itself
/// measures rows from their rendered elements.
const ROW_HEIGHT_PX: f32 = 28.0;
const PAGE_ROWS: usize = 10;

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

        let curated_actions = curated_action_set();
        let (local_bindings, global_bindings) =
            collect_bindings(&pane_context_stack, &curated_actions, raw_bindings, cx);

        let rows = build_rows(
            leaf_context_label.clone(),
            &local_bindings,
            &global_bindings,
        );
        let rows: Rc<[RowKind]> = Rc::from(rows);

        // Overdraw ~3 screens worth of rows so scrolling never reveals
        // a blank gap before measurement catches up.
        let overdraw = px(ROW_HEIGHT_PX * (PAGE_ROWS as f32) * 3.0);
        let list_state = ListState::new(rows.len(), ListAlignment::Top, overdraw);

        Self {
            focus_handle,
            rows,
            list_state,
            cursor: 0,
            dismissed: false,
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let key = event.keystroke.key.as_str();
        let shift = event.keystroke.modifiers.shift;
        let ctrl = event.keystroke.modifiers.control;
        let last = self.rows.len().saturating_sub(1);

        let mut handled = true;
        match key {
            "escape" => {
                self.dismissed = true;
                cx.emit(DismissEvent);
                return;
            }
            "j" | "down" => self.move_cursor(1, false),
            "k" | "up" => self.move_cursor(-1, false),
            "pagedown" => self.move_cursor(PAGE_ROWS as isize, true),
            "pageup" => self.move_cursor(-(PAGE_ROWS as isize), true),
            "d" if ctrl => self.move_cursor((PAGE_ROWS / 2) as isize, true),
            "u" if ctrl => self.move_cursor(-((PAGE_ROWS / 2) as isize), true),
            "home" => self.set_cursor(0),
            "g" if !shift => self.set_cursor(0),
            "g" if shift => self.set_cursor(last),
            "end" => self.set_cursor(last),
            "enter" => {
                self.dispatch_cursor(window, cx);
                return;
            }
            _ => handled = false,
        }
        if handled {
            cx.notify();
        }
    }

    /// Step the cursor by `delta` rows, skipping over non-binding rows
    /// (headers / hints) so j/k always lands on something dispatchable.
    fn move_cursor(&mut self, delta: isize, snap_after: bool) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        let mut next = self.cursor as isize + delta;
        next = next.clamp(0, last);
        // If we landed on a header / hint, slide in the direction of
        // travel until we hit a Pair (or run off the end).
        let step: isize = if delta >= 0 { 1 } else { -1 };
        while next >= 0 && next <= last {
            if matches!(self.rows[next as usize], RowKind::Pair { .. }) {
                break;
            }
            next += step;
        }
        if !(0..=last).contains(&next) {
            // No pair in that direction — undo.
            return;
        }
        self.cursor = next as usize;
        // Page jumps reveal directly; single-step jumps just ensure
        // the cursor stays visible.
        if snap_after {
            self.list_state.scroll_to_reveal_item(self.cursor);
        } else {
            self.list_state.scroll_to_reveal_item(self.cursor);
        }
    }

    fn set_cursor(&mut self, target: usize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len().saturating_sub(1);
        let target = target.min(last);
        // Snap to the nearest Pair in either direction so the highlight
        // never lands on a header.
        let mut chosen = None;
        for offset in 0..=last {
            for sign in [1isize, -1] {
                let ix = target as isize + sign * offset as isize;
                if ix < 0 || ix as usize > last {
                    continue;
                }
                if matches!(self.rows[ix as usize], RowKind::Pair { .. }) {
                    chosen = Some(ix as usize);
                    break;
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        if let Some(ix) = chosen {
            self.cursor = ix;
            self.list_state.scroll_to_reveal_item(self.cursor);
        }
    }

    /// Dispatch the action under the cursor as if the user had pressed
    /// the binding directly. Dismisses the modal first so the action
    /// lands on the right pane.
    fn dispatch_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(RowKind::Pair { left, .. }) = self.rows.get(self.cursor).cloned() else {
            return;
        };
        let action_name = left.raw_action_name.to_string();
        self.dismissed = true;
        cx.emit(DismissEvent);
        // Best-effort dispatch by name. Failures (unknown action, missing
        // data) just dismiss silently — the user can still type the
        // binding directly.
        if let Ok(action) = cx.build_action(&action_name, None) {
            window.dispatch_action(action, cx);
        }
    }
}

fn curated_action_set() -> HashSet<String> {
    crate::keymap::codon_default_bindings()
        .into_iter()
        .chain(crate::keymap::codon_user_bindings())
        .map(|(_, action, _)| action)
        .collect()
}

fn build_rows(
    leaf_context_label: Option<SharedString>,
    local: &[BindingRow],
    global: &[BindingRow],
) -> Vec<RowKind> {
    let mut rows: Vec<RowKind> = Vec::new();

    let local_label = leaf_context_label
        .clone()
        .map(|leaf| SharedString::from(format!("This pane · {leaf}")))
        .unwrap_or_else(|| SharedString::from("This pane"));
    rows.push(RowKind::Header {
        label: local_label,
        count: local.len(),
    });
    if local.is_empty() {
        rows.push(RowKind::EmptyHint(SharedString::from(
            "No pane-specific bindings",
        )));
    } else {
        append_pairs(&mut rows, local);
    }

    if !global.is_empty() {
        rows.push(RowKind::Header {
            label: SharedString::from("Global"),
            count: global.len(),
        });
        append_pairs(&mut rows, global);
    }

    rows
}

/// Lay out `items` top-down-then-right into a column of paired rows.
/// Striping is anchored on the *pair index* (not the column slot) so
/// backgrounds stay put across scroll positions.
fn append_pairs(out: &mut Vec<RowKind>, items: &[BindingRow]) {
    let n = items.len();
    if n == 0 {
        return;
    }
    let split = n.div_ceil(2);
    for pair_index in 0..split {
        let left = items[pair_index].clone();
        let right_index = pair_index + split;
        let right = items.get(right_index).cloned();
        out.push(RowKind::Pair {
            left,
            right,
            striped: pair_index % 2 == 1,
        });
    }
}

fn render_binding_cell(row: &BindingRow, is_cursor: bool) -> AnyElement {
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
                .color(if is_cursor {
                    Color::Default
                } else {
                    Color::Default
                })
                .size(LabelSize::Default)
                .single_line()
                .truncate(),
        )
        .into_any_element()
}

fn render_pair(
    left: &BindingRow,
    right: Option<&BindingRow>,
    striped: bool,
    is_cursor: bool,
    row_bg: Hsla,
    cursor_bg: Hsla,
    accent: Hsla,
) -> AnyElement {
    let bg = if is_cursor {
        Some(cursor_bg)
    } else if striped {
        Some(row_bg)
    } else {
        None
    };
    let left_cell = h_flex()
        .items_center()
        .px_2()
        .py_0p5()
        .rounded_md()
        .flex_1()
        .min_w(px(0.))
        .when_some(bg, |el, c| el.bg(c))
        .when(is_cursor, |el| el.border_l_2().border_color(accent))
        .child(render_binding_cell(left, is_cursor));
    let right_cell: AnyElement = match right {
        Some(binding) => h_flex()
            .items_center()
            .px_2()
            .py_0p5()
            .rounded_md()
            .flex_1()
            .min_w(px(0.))
            .when_some(bg, |el, c| el.bg(c))
            .child(render_binding_cell(binding, false))
            .into_any_element(),
        None => div().flex_1().min_w(px(0.)).into_any_element(),
    };
    h_flex()
        .w_full()
        .gap_6()
        .items_center()
        .child(left_cell)
        .child(right_cell)
        .into_any_element()
}

fn render_header(
    label: SharedString,
    count: usize,
    accent: Hsla,
    divider: Hsla,
) -> AnyElement {
    let header_row = h_flex()
        .items_center()
        .gap_2()
        .pt_3()
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
    v_flex()
        .w_full()
        .child(header_row)
        .child(div().h(px(1.)).w_full().bg(divider))
        .into_any_element()
}

fn render_empty_hint(text: SharedString) -> AnyElement {
    v_flex()
        .py_1()
        .child(
            Label::new(text)
                .color(Color::Muted)
                .size(LabelSize::Small),
        )
        .into_any_element()
}

fn leaf_context_label(stack: &[KeyContext]) -> Option<SharedString> {
    let leaf = stack.last()?;
    leaf.primary().map(|entry| entry.key.clone())
}

fn collect_bindings(
    pane_context_stack: &[KeyContext],
    curated_actions: &HashSet<String>,
    raw: Vec<GpuiKeyBinding>,
    cx: &mut Context<KeybindingsCheatsheetModal>,
) -> (Vec<BindingRow>, Vec<BindingRow>) {
    // `raw` is captured pre-modal so it includes pane-specific bindings
    // (`Terminal && pane_mode == normal`, …) that would otherwise be
    // filtered out by the time `possible_bindings_for_input` ran inside
    // `new`. Bindings are ordered by precedence — deeper context first,
    // most-recently-registered first. User overrides therefore appear
    // before the corresponding default. Collapse all bindings that share
    // a `(chord, context)` pair to the first occurrence so the cheatsheet
    // shows what would actually fire, never both.
    let mut local: Vec<BindingRow> = Vec::new();
    let mut global: Vec<BindingRow> = Vec::with_capacity(raw.len());
    let mut seen: HashSet<(SharedString, String)> = HashSet::with_capacity(raw.len());
    for binding in raw.iter() {
        let raw_name = binding.action().name();
        // Only show bindings codon itself owns — the curated set built
        // from `DEFAULT_KEYMAP` + `~/.config/codon/codon.toml`.
        if !curated_actions.contains(raw_name) {
            continue;
        }
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
        let humanized = command_palette::humanize_action_name(raw_name);
        let row = BindingRow {
            keystrokes: Rc::from(keystrokes),
            keystrokes_text,
            action_name: SharedString::from(humanized),
            raw_action_name: SharedString::from(raw_name),
        };
        // A binding is *pane-local* iff its predicate is satisfied by the
        // full stack but NOT by the stack with the leaf removed — i.e.
        // the leaf context is load-bearing. Plain "matches at leaf depth"
        // isn't enough because predicates like `!Editor`, `Pane &&
        // something`, or anything that's true at multiple levels also
        // satisfies `depth_of == stack.len()` and would flood the
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
    let sort = |a: &BindingRow, b: &BindingRow| {
        chord_sort_key(&a.keystrokes_text)
            .cmp(&chord_sort_key(&b.keystrokes_text))
            .then_with(|| a.action_name.cmp(&b.action_name))
    };
    local.sort_by(sort);
    global.sort_by(sort);
    (local, global)
}

/// Sort by chord length (shorter first), then by text. So `cmd-k a` sorts
/// before `cmd-k a a`, and bindings without a chord prefix come first.
fn chord_sort_key(text: &str) -> (usize, String) {
    (text.split_whitespace().count(), text.to_string())
}

impl Render for KeybindingsCheatsheetModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let theme = cx.theme();
        let panel_bg = theme.colors().elevated_surface_background;
        let row_bg = theme.colors().surface_background;
        let cursor_bg = theme.colors().element_selected;
        let accent = theme.colors().text_accent;
        let border = theme.colors().border;
        let divider = theme.colors().border_variant;

        let mut key_context = KeyContext::default();
        key_context.add("KeybindingsCheatsheet");
        key_context.add("menu");

        // Short-circuit paint during fade-out so the modal layer's
        // animation frames don't pay any layout cost.
        if self.dismissed {
            return div()
                .key_context(key_context)
                .track_focus(&self.focus_handle)
                .size_full();
        }

        let total_count = self
            .rows
            .iter()
            .filter(|r| {
                matches!(
                    r,
                    RowKind::Pair {
                        right: Some(_),
                        ..
                    } | RowKind::Pair { right: None, .. }
                )
            })
            .map(|r| match r {
                RowKind::Pair { right: Some(_), .. } => 2,
                RowKind::Pair { right: None, .. } => 1,
                _ => 0,
            })
            .sum::<usize>();

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
                            "{} curated · j/k move · Enter run · Esc dismiss",
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

        let rows = self.rows.clone();
        let cursor = self.cursor;
        let list_state = self.list_state.clone();
        let body = list(
            list_state,
            move |ix, _window, _cx| match rows.get(ix) {
                Some(RowKind::Header { label, count }) => {
                    render_header(label.clone(), *count, accent, divider)
                }
                Some(RowKind::EmptyHint(text)) => render_empty_hint(text.clone()),
                Some(RowKind::Pair {
                    left,
                    right,
                    striped,
                }) => render_pair(
                    left,
                    right.as_ref(),
                    *striped,
                    ix == cursor,
                    row_bg,
                    cursor_bg,
                    accent,
                ),
                None => div().into_any_element(),
            },
        )
        .flex_grow();

        let max_w = px((f32::from(viewport.width) * 0.85).min(960.));
        let max_h = px(f32::from(viewport.height) * 0.85);

        div()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
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
                    .child(body),
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
