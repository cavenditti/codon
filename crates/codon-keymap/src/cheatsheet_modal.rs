//! Full-screen modal listing every keybinding codon ships or the user
//! configures. Bound to `cmd-k F1` by default.
//!
//! Only bindings declared in codon's curated set are shown — that's
//! everything in the embedded `DEFAULT_KEYMAP` plus everything the user
//! added to `~/.config/codon/codon.toml`. Vendor/zed's ~1000+ upstream
//! defaults are filtered out: they're noise to a codon user.
//!
//! UX:
//!
//! - `Tab` / `Shift-Tab` cycle between the two sets of bindings —
//!   "This pane" (context-local) and "Global" (everything else).
//! - `/` enters filter mode; typing characters narrows the visible set
//!   by chord or action name. `Esc` cancels the filter without
//!   dismissing the modal.
//! - `j` / `k` (and arrows) move the cursor; `Enter` dispatches the
//!   cursored action; `Esc` dismisses.

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheatTab {
    ThisPane,
    Global,
}

impl CheatTab {
    fn label(self) -> &'static str {
        match self {
            CheatTab::ThisPane => "This pane",
            CheatTab::Global => "Global",
        }
    }
    fn next(self) -> Self {
        match self {
            CheatTab::ThisPane => CheatTab::Global,
            CheatTab::Global => CheatTab::ThisPane,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheatMode {
    Browse,
    Filter,
}

pub struct KeybindingsCheatsheetModal {
    focus_handle: FocusHandle,
    /// Captured at open time so the "This pane" tab can name the pane
    /// kind (e.g. "This pane · Terminal").
    leaf_context_label: Option<SharedString>,
    local_bindings: Vec<BindingRow>,
    global_bindings: Vec<BindingRow>,
    tab: CheatTab,
    mode: CheatMode,
    filter: String,
    /// Visible rows after tab + filter — interleaved pair rows / empty
    /// hints. `gpui::list` virtualizes against this slice.
    rows: Rc<[RowKind]>,
    list_state: ListState,
    /// Cursor index into `rows`. `j` / `k` bump it; the cursored row gets
    /// a subtle highlight.
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
    /// One-line muted hint, used when the active tab + filter combo has
    /// no matching bindings.
    EmptyHint(SharedString),
    /// One pair of side-by-side bindings. `right` is `None` for the last
    /// row in an odd-count list; the renderer fills the missing column
    /// with a spacer to keep column widths stable.
    Pair {
        left: BindingRow,
        right: Option<BindingRow>,
        striped: bool,
    },
}

const ROW_HEIGHT_PX: f32 = 28.0;
const PAGE_ROWS: usize = 12;
/// Modal height fraction of the viewport. The user explicitly asked for
/// a taller cheatsheet — most viewports give ~720+ px of body at 0.92.
const MODAL_H_FRAC: f32 = 0.92;
const MODAL_W_FRAC: f32 = 0.85;
const MODAL_MAX_W: f32 = 1080.0;
const MODAL_MIN_H: f32 = 520.0;

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

        // Open on whichever tab actually has bindings to show — if the
        // current pane has no context-local bindings, jump straight to
        // Global instead of greeting the user with an empty page.
        let tab = if local_bindings.is_empty() {
            CheatTab::Global
        } else {
            CheatTab::ThisPane
        };

        let overdraw = px(ROW_HEIGHT_PX * (PAGE_ROWS as f32) * 3.0);
        let mut this = Self {
            focus_handle,
            leaf_context_label,
            local_bindings,
            global_bindings,
            tab,
            mode: CheatMode::Browse,
            filter: String::new(),
            rows: Rc::from(Vec::<RowKind>::new()),
            list_state: ListState::new(0, ListAlignment::Top, overdraw),
            cursor: 0,
            dismissed: false,
        };
        this.rebuild_rows();
        this
    }

    fn rebuild_rows(&mut self) {
        let source: &[BindingRow] = match self.tab {
            CheatTab::ThisPane => &self.local_bindings,
            CheatTab::Global => &self.global_bindings,
        };
        let filtered: Vec<BindingRow> = if self.filter.is_empty() {
            source.to_vec()
        } else {
            let needle = self.filter.to_ascii_lowercase();
            source
                .iter()
                .filter(|r| {
                    r.action_name.to_ascii_lowercase().contains(&needle)
                        || r.keystrokes_text.to_ascii_lowercase().contains(&needle)
                        || r.raw_action_name.to_ascii_lowercase().contains(&needle)
                })
                .cloned()
                .collect()
        };

        let mut rows: Vec<RowKind> = Vec::new();
        if filtered.is_empty() {
            let hint = if !self.filter.is_empty() {
                SharedString::from(format!("No bindings match `{}`", self.filter))
            } else if matches!(self.tab, CheatTab::ThisPane) {
                SharedString::from("No bindings specific to this pane")
            } else {
                SharedString::from("No bindings configured")
            };
            rows.push(RowKind::EmptyHint(hint));
        } else {
            append_pairs(&mut rows, &filtered);
        }

        self.rows = Rc::from(rows);
        self.list_state.reset(self.rows.len());
        self.cursor = first_pair_index(&self.rows).unwrap_or(0);
    }

    fn cycle_tab(&mut self, forward: bool) {
        // With only two tabs forward and backward are the same; kept as
        // an argument so adding a third set later is purely additive.
        let _ = forward;
        self.tab = self.tab.next();
        self.rebuild_rows();
    }

    fn enter_filter(&mut self) {
        self.mode = CheatMode::Filter;
    }

    fn exit_filter(&mut self, clear: bool) {
        self.mode = CheatMode::Browse;
        if clear && !self.filter.is_empty() {
            self.filter.clear();
            self.rebuild_rows();
        }
    }

    fn push_filter_char(&mut self, ch: &str) {
        if ch.chars().any(|c| c.is_control()) {
            return;
        }
        self.filter.push_str(ch);
        self.rebuild_rows();
    }

    fn pop_filter_char(&mut self) {
        if self.filter.pop().is_some() {
            self.rebuild_rows();
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
        let alt = event.keystroke.modifiers.alt;
        let cmd = event.keystroke.modifiers.platform;

        match self.mode {
            CheatMode::Browse => self.handle_browse_key(event, key, shift, ctrl, window, cx),
            CheatMode::Filter => {
                self.handle_filter_key(event, key, shift, ctrl, alt, cmd, cx);
            }
        }
    }

    fn handle_browse_key(
        &mut self,
        _event: &KeyDownEvent,
        key: &str,
        shift: bool,
        ctrl: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let last = self.rows.len().saturating_sub(1);
        let mut handled = true;
        match key {
            "escape" => {
                self.dismissed = true;
                cx.emit(DismissEvent);
                return;
            }
            "tab" => self.cycle_tab(!shift),
            "/" => self.enter_filter(),
            "j" | "down" => self.move_cursor(1),
            "k" | "up" => self.move_cursor(-1),
            "pagedown" => self.move_cursor(PAGE_ROWS as isize),
            "pageup" => self.move_cursor(-(PAGE_ROWS as isize)),
            "d" if ctrl => self.move_cursor((PAGE_ROWS / 2) as isize),
            "u" if ctrl => self.move_cursor(-((PAGE_ROWS / 2) as isize)),
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

    fn handle_filter_key(
        &mut self,
        event: &KeyDownEvent,
        key: &str,
        shift: bool,
        ctrl: bool,
        alt: bool,
        cmd: bool,
        cx: &mut Context<Self>,
    ) {
        let mut handled = true;
        match key {
            "escape" => self.exit_filter(true),
            "enter" => self.exit_filter(false),
            "tab" => {
                // Switching tabs while filtering carries the filter
                // across so the user can see how it matches in both sets.
                self.cycle_tab(!shift);
            }
            "backspace" => self.pop_filter_char(),
            "down" => self.move_cursor(1),
            "up" => self.move_cursor(-1),
            _ => {
                handled = false;
                if cmd || ctrl || alt {
                    // Don't try to fold modifier chords into the filter
                    // — let them fall through (currently no-op).
                } else if let Some(ch) = event.keystroke.key_char.as_deref() {
                    if !ch.is_empty() {
                        self.push_filter_char(ch);
                        handled = true;
                    }
                } else if key == "space" {
                    self.push_filter_char(" ");
                    handled = true;
                } else if !shift && key.chars().count() == 1 {
                    self.push_filter_char(key);
                    handled = true;
                }
            }
        }
        if handled {
            cx.notify();
        }
    }

    /// Step the cursor by `delta` rows, skipping over non-binding rows
    /// (headers / hints) so j/k always lands on something dispatchable.
    fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        let mut next = self.cursor as isize + delta;
        next = next.clamp(0, last);
        let step: isize = if delta >= 0 { 1 } else { -1 };
        while (0..=last).contains(&next) {
            if matches!(self.rows[next as usize], RowKind::Pair { .. }) {
                break;
            }
            next += step;
        }
        if !(0..=last).contains(&next) {
            return;
        }
        self.cursor = next as usize;
        self.list_state.scroll_to_reveal_item(self.cursor);
    }

    fn set_cursor(&mut self, target: usize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len().saturating_sub(1);
        let target = target.min(last);
        let chosen = nearest_pair(&self.rows, target);
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
        if let Ok(action) = cx.build_action(&action_name, None) {
            window.dispatch_action(action, cx);
        }
    }
}

fn first_pair_index(rows: &[RowKind]) -> Option<usize> {
    rows.iter()
        .position(|r| matches!(r, RowKind::Pair { .. }))
}

fn nearest_pair(rows: &[RowKind], target: usize) -> Option<usize> {
    let last = rows.len().saturating_sub(1);
    for offset in 0..=last {
        for sign in [1isize, -1] {
            let ix = target as isize + sign * offset as isize;
            if ix < 0 || ix as usize > last {
                continue;
            }
            if matches!(rows[ix as usize], RowKind::Pair { .. }) {
                return Some(ix as usize);
            }
        }
    }
    None
}

fn curated_action_set() -> HashSet<String> {
    crate::keymap::codon_default_bindings()
        .into_iter()
        .chain(crate::keymap::codon_user_bindings())
        .map(|(_, action, _)| action)
        .collect()
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
        .child(render_binding_cell(left));
    let right_cell: AnyElement = match right {
        Some(binding) => h_flex()
            .items_center()
            .px_2()
            .py_0p5()
            .rounded_md()
            .flex_1()
            .min_w(px(0.))
            .when_some(bg, |el, c| el.bg(c))
            .child(render_binding_cell(binding))
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

fn render_empty_hint(text: SharedString) -> AnyElement {
    v_flex()
        .py_8()
        .child(
            Label::new(text)
                .color(Color::Muted)
                .size(LabelSize::Default),
        )
        .into_any_element()
}

/// Render the two-tab pill bar with a count badge per tab.
fn render_tabs(
    active: CheatTab,
    leaf_label: Option<&SharedString>,
    local_count: usize,
    global_count: usize,
    accent: Hsla,
    pill_bg: Hsla,
) -> AnyElement {
    let this_pane_label = match leaf_label {
        Some(leaf) => SharedString::from(format!("This pane · {leaf}")),
        None => SharedString::from(CheatTab::ThisPane.label()),
    };

    let tab = |label: SharedString, count: usize, is_active: bool| {
        let pill = h_flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .rounded_md()
            .when(is_active, |el| el.bg(pill_bg))
            .child(
                Label::new(label)
                    .color(if is_active { Color::Default } else { Color::Muted })
                    .size(LabelSize::Default)
                    .weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    }),
            )
            .child(
                Label::new(format!("{count}"))
                    .color(if is_active { Color::Accent } else { Color::Muted })
                    .size(LabelSize::Small),
            )
            .when(is_active, |el| el.border_b_2().border_color(accent));
        pill.into_any_element()
    };

    h_flex()
        .gap_1()
        .items_center()
        .child(tab(
            this_pane_label,
            local_count,
            matches!(active, CheatTab::ThisPane),
        ))
        .child(tab(
            SharedString::from(CheatTab::Global.label()),
            global_count,
            matches!(active, CheatTab::Global),
        ))
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
        // the leaf context is load-bearing.
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
        let pill_bg = theme.colors().element_background;
        let accent = theme.colors().text_accent;
        let border = theme.colors().border;
        let divider = theme.colors().border_variant;

        let mut key_context = KeyContext::default();
        key_context.add("KeybindingsCheatsheet");
        key_context.add("menu");

        // Short-circuit paint during fade-out so the modal layer's
        // animation frames pay no layout cost.
        if self.dismissed {
            return div()
                .key_context(key_context)
                .track_focus(&self.focus_handle)
                .size_full();
        }

        let visible_count: usize = self
            .rows
            .iter()
            .map(|r| match r {
                RowKind::Pair { right: Some(_), .. } => 2,
                RowKind::Pair { right: None, .. } => 1,
                _ => 0,
            })
            .sum();
        let total_in_tab = match self.tab {
            CheatTab::ThisPane => self.local_bindings.len(),
            CheatTab::Global => self.global_bindings.len(),
        };
        let count_text = if self.filter.is_empty() {
            format!("{visible_count} bindings")
        } else {
            format!("{visible_count} of {total_in_tab} match")
        };

        let help_text = match self.mode {
            CheatMode::Browse => "Tab switch · / filter · j/k move · Enter run · Esc dismiss",
            CheatMode::Filter => "type to filter · ↑/↓ move · Enter confirm · Esc clear filter",
        };

        let title_block = v_flex()
            .gap_0p5()
            .child(Headline::new("Keybindings").size(HeadlineSize::Medium))
            .child(
                Label::new(count_text)
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            );

        let header_top = h_flex()
            .items_center()
            .justify_between()
            .pb_3()
            .child(title_block)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Label::new("⌘ K  F1").color(Color::Muted).size(LabelSize::Small))
                    .child(ui::Icon::new(IconName::Command).color(Color::Muted)),
            );

        let tab_bar = h_flex()
            .items_center()
            .justify_between()
            .pb_2()
            .child(render_tabs(
                self.tab,
                self.leaf_context_label.as_ref(),
                self.local_bindings.len(),
                self.global_bindings.len(),
                accent,
                pill_bg,
            ))
            .child(
                Label::new(help_text)
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            );

        let rows = self.rows.clone();
        let cursor = self.cursor;
        let list_state = self.list_state.clone();
        let body = list(
            list_state,
            move |ix, _window, _cx| match rows.get(ix) {
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

        let filter_bar = if matches!(self.mode, CheatMode::Filter) || !self.filter.is_empty() {
            let prompt = h_flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1p5()
                .rounded_md()
                .bg(theme.colors().editor_background)
                .border_1()
                .border_color(if matches!(self.mode, CheatMode::Filter) {
                    accent
                } else {
                    divider
                })
                .child(
                    Label::new(SharedString::from("/"))
                        .color(Color::Muted)
                        .size(LabelSize::Default),
                )
                .child(
                    Label::new(SharedString::from(self.filter.clone()))
                        .color(Color::Default)
                        .size(LabelSize::Default),
                )
                .when(matches!(self.mode, CheatMode::Filter), |el| {
                    // Visual cursor: a thin accent bar after the typed
                    // text. Static (no blink) — the modal already conveys
                    // enough state.
                    el.child(div().w(px(2.)).h(px(14.)).bg(accent).rounded_full())
                });
            Some(prompt)
        } else {
            None
        };

        let max_w = px((f32::from(viewport.width) * MODAL_W_FRAC).min(MODAL_MAX_W));
        let max_h = px(f32::from(viewport.height) * MODAL_H_FRAC);

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
                    .min_h(px(MODAL_MIN_H))
                    .rounded_lg()
                    .bg(panel_bg)
                    .border_1()
                    .border_color(border)
                    .shadow_lg()
                    .px_6()
                    .py_5()
                    .child(header_top)
                    .child(tab_bar)
                    .child(div().h(px(1.)).w_full().bg(divider).mb_2())
                    .child(body)
                    .when_some(filter_bar, |el, bar| el.child(div().mt_2().child(bar))),
            )
    }
}

impl EventEmitter<DismissEvent> for KeybindingsCheatsheetModal {}

impl Focusable for KeybindingsCheatsheetModal {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl codon_mode::PaneModeBridge for KeybindingsCheatsheetModal {
    fn pane_mode(&self) -> codon_mode::PaneMode {
        codon_mode::PaneMode::Normal
    }

    fn command_active_override(&self) -> Option<bool> {
        // Explicitly clear the COMMAND flag on focus so the
        // cheatsheet doesn't inherit a still-set `command_active`
        // from a recently-closed palette underneath. The cheatsheet
        // is a read-only keybinding browser, not a command-class
        // modal.
        Some(false)
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
