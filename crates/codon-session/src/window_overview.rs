use gpui::{
    AnyElement, App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
};
use ui::{
    ActiveTheme as _, Color, FluentBuilder as _, Icon, IconName, IconSize, Label, LabelCommon as _,
    LabelSize, StyledExt as _, div, h_flex, v_flex,
};
use workspace::{
    ModalView, Workspace,
    codon_bridge::{LayoutSnapshot, PaneSnapshot, SnapshotAxis},
};

use crate::{
    registry::SessionRegistry,
    session::{Window as SessionWindow, WindowId},
    window_indicator::switch_to_window,
};

const TILE_WIDTH: f32 = 240.0;
const TILE_HEIGHT: f32 = 150.0;
const SKETCH_WIDTH: f32 = 200.0;
const SKETCH_HEIGHT: f32 = 64.0;
const TILE_GAP: f32 = 12.0;
const MODAL_PAD: f32 = 32.0;
/// Viewport fraction the overview is allowed to consume.
const MODAL_W_FRAC: f32 = 0.85;
const MODAL_H_FRAC: f32 = 0.85;
/// Used only as a pre-render fallback before `render()` reads the viewport.
const COLUMNS_FALLBACK: usize = 3;

/// Visible columns given a viewport width. Mirrors session-overview's
/// computation so h/j/k/l feels consistent across both overviews.
fn columns_for_viewport(viewport_w: f32) -> usize {
    let usable = (viewport_w * MODAL_W_FRAC - MODAL_PAD).max(TILE_WIDTH);
    let cols = (usable / (TILE_WIDTH + TILE_GAP)).floor() as usize;
    cols.clamp(2, 6)
}

pub struct WindowOverviewModal {
    workspace: gpui::WeakEntity<Workspace>,
    focus: FocusHandle,
    /// Snapshot of windows taken at modal-open time. Stable indices keep
    /// keyboard navigation simple — we don't re-sort while open.
    windows: Vec<SessionWindow>,
    active_window_id: Option<WindowId>,
    selected: usize,
    columns: usize,
}

impl WindowOverviewModal {
    pub fn new(
        workspace: gpui::WeakEntity<Workspace>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let registry = SessionRegistry::global(cx);
        let (windows, active_window_id, selected) = match registry.active() {
            Some(session) => {
                let windows = session.windows.clone();
                let active_id = windows.get(session.active_window).map(|w| w.id);
                (windows, active_id, session.active_window)
            }
            None => (Vec::new(), None, 0),
        };

        Self {
            workspace,
            focus: cx.focus_handle(),
            windows,
            active_window_id,
            selected,
            columns: COLUMNS_FALLBACK,
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.windows.is_empty() {
            if event.keystroke.key == "escape" {
                cx.emit(DismissEvent);
            }
            return;
        }

        let key = event.keystroke.key.as_str();
        let cols = self.columns.max(1);
        let len = self.windows.len();
        let mut handled = true;

        match key {
            "h" | "left" => {
                if self.selected % cols > 0 {
                    self.selected -= 1;
                }
            }
            "l" | "right" => {
                if self.selected + 1 < len && (self.selected + 1) % cols != 0 {
                    self.selected += 1;
                }
            }
            "k" | "up" => {
                if self.selected >= cols {
                    self.selected -= cols;
                }
            }
            "j" | "down" => {
                if self.selected + cols < len {
                    self.selected += cols;
                }
            }
            "enter" => {
                self.switch_selected(window, cx);
                return;
            }
            "escape" => {
                cx.emit(DismissEvent);
                return;
            }
            _ => handled = false,
        }

        if handled {
            cx.notify();
        }
    }

    fn switch_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.windows.get(self.selected).map(|w| w.id) else {
            cx.emit(DismissEvent);
            return;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            cx.emit(DismissEvent);
            return;
        };
        workspace.update(cx, |workspace, cx| {
            switch_to_window(workspace, target, window, cx);
        });
        cx.emit(DismissEvent);
    }

    fn render_tile(
        &self,
        index: usize,
        win: &SessionWindow,
        cx: &App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_active = Some(win.id) == self.active_window_id;
        let is_selected = index == self.selected;

        let border_color = if is_active {
            theme.colors().text_accent
        } else if is_selected {
            theme.colors().border_focused
        } else {
            theme.colors().border_variant
        };

        let bg = if is_selected {
            theme.colors().element_selected
        } else {
            theme.colors().elevated_surface_background
        };

        let dominant = dominant_pane_kind(win.layout.as_ref());
        let icon_name = icon_for_kind(dominant.as_deref());
        let pane_count = count_panes(win.layout.as_ref());

        let sketch = render_sketch(win.layout.as_ref(), cx);

        v_flex()
            .id(("codon-window-overview-tile", index))
            .w(gpui::px(TILE_WIDTH))
            .h(gpui::px(TILE_HEIGHT))
            .p_3()
            .gap_1()
            .rounded_md()
            .border_2()
            .border_color(border_color)
            .bg(bg)
            .child(
                h_flex()
                    .gap_2()
                    .child(Icon::new(icon_name).size(IconSize::Small))
                    .child(
                        Label::new(win.name.clone())
                            .size(LabelSize::Large)
                            .when(is_active, |label| label.color(Color::Accent)),
                    ),
            )
            .child(sketch)
            .child(
                Label::new(format!(
                    "{} pane{}",
                    pane_count,
                    if pane_count == 1 { "" } else { "s" }
                ))
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
    }
}

impl EventEmitter<DismissEvent> for WindowOverviewModal {}
impl ModalView for WindowOverviewModal {}

impl Focusable for WindowOverviewModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WindowOverviewModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let viewport_w = f32::from(viewport.width);
        let viewport_h = f32::from(viewport.height);
        // Snap the keyboard-nav column count to whatever fits the visible
        // grid so h/j/k/l matches what the user sees.
        self.columns = columns_for_viewport(viewport_w);
        let max_w = gpui::px((viewport_w * MODAL_W_FRAC).min(1200.0));
        let max_h = gpui::px(viewport_h * MODAL_H_FRAC);
        let theme = cx.theme();

        let tiles: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .map(|(ix, win)| self.render_tile(ix, win, cx).into_any_element())
            .collect();

        let header = h_flex()
            .w_full()
            .justify_between()
            .px_1()
            .pb_2()
            .child(Label::new("Windows").size(LabelSize::Large))
            .child(
                Label::new("h/j/k/l move · Enter switch · Esc dismiss")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );

        let body = if self.windows.is_empty() {
            div()
                .p_8()
                .child(
                    Label::new("No windows — `cmd-k shift-w n` to add one.")
                        .color(Color::Muted),
                )
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap_3()
                .children(tiles)
                .into_any_element()
        };

        v_flex()
            .key_context("CodonWindowOverview")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::handle_key_down))
            .elevation_3(cx)
            .max_w(max_w)
            .max_h(max_h)
            .p_4()
            .gap_2()
            .bg(theme.colors().elevated_surface_background)
            .child(header)
            .child(body)
    }
}

/// Return the most common pane "kind" in the layout, breaking ties by
/// first occurrence. Returns `None` for an empty / missing layout so the
/// caller can fall back to a generic icon.
fn dominant_pane_kind(layout: Option<&LayoutSnapshot>) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    let Some(layout) = layout else {
        return None;
    };
    collect_pane_kinds(layout, &mut counts);
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(k, _)| k)
}

fn collect_pane_kinds(layout: &LayoutSnapshot, counts: &mut Vec<(String, usize)>) {
    match layout {
        LayoutSnapshot::Group { children, .. } => {
            for child in children {
                collect_pane_kinds(child, counts);
            }
        }
        LayoutSnapshot::Stack { members, active } => {
            if let Some(child) = members.get(*active) {
                collect_pane_kinds(child, counts);
            }
        }
        LayoutSnapshot::Pane(pane) => {
            let kind = active_item_kind(pane).unwrap_or("empty".to_string());
            if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == kind) {
                entry.1 += 1;
            } else {
                counts.push((kind, 1));
            }
        }
    }
}

fn active_item_kind(pane: &PaneSnapshot) -> Option<String> {
    pane.items
        .iter()
        .find(|item| item.active)
        .or_else(|| pane.items.first())
        .map(|item| item.kind.clone())
}

fn icon_for_kind(kind: Option<&str>) -> IconName {
    match kind {
        Some("Terminal") => IconName::Terminal,
        Some("Editor") => IconName::Code,
        // file-manager and agent live in panels today, so they won't
        // generally appear in pane items — but be forgiving in case a
        // future change exposes them as items.
        Some("FileManager") | Some("file-manager") => IconName::FolderOpen,
        Some("Agent") | Some("agent") | Some("AgentPanel") => IconName::ZedAssistant,
        Some("Image") | Some("ImageView") => IconName::Image,
        _ => IconName::Terminal,
    }
}

fn count_panes(layout: Option<&LayoutSnapshot>) -> usize {
    fn walk(node: &LayoutSnapshot) -> usize {
        match node {
            LayoutSnapshot::Group { children, .. } => children.iter().map(walk).sum(),
            LayoutSnapshot::Stack { members, active } => {
                members.get(*active).map(walk).unwrap_or(1)
            }
            LayoutSnapshot::Pane(_) => 1,
        }
    }
    layout.map(walk).unwrap_or(1)
}

fn render_sketch(layout: Option<&LayoutSnapshot>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let outer = div()
        .w(gpui::px(SKETCH_WIDTH))
        .h(gpui::px(SKETCH_HEIGHT))
        .border_1()
        .border_color(theme.colors().border_variant)
        .rounded_sm()
        .bg(theme.colors().surface_background);

    match layout {
        None => outer
            .child(
                div()
                    .size_full()
                    .child(div().size_full().bg(theme.colors().element_background)),
            )
            .into_any_element(),
        Some(snapshot) => outer
            .child(render_sketch_node(snapshot, cx))
            .into_any_element(),
    }
}

fn render_sketch_node(node: &LayoutSnapshot, cx: &App) -> AnyElement {
    let theme = cx.theme();
    match node {
        LayoutSnapshot::Pane(pane) => {
            // The active pane within the active window gets a slightly
            // brighter fill; everything else uses a muted surface so the
            // split shape reads without overpowering the tile.
            let fill = if pane.active {
                theme.colors().element_background
            } else {
                theme.colors().surface_background
            };
            div()
                .size_full()
                .border_1()
                .border_color(theme.colors().border_variant)
                .bg(fill)
                .into_any_element()
        }
        LayoutSnapshot::Stack { members, active } => {
            // Stacks (tabs) collapse to whichever child is visible.
            match members.get(*active) {
                Some(child) => render_sketch_node(child, cx),
                None => div()
                    .size_full()
                    .bg(theme.colors().surface_background)
                    .into_any_element(),
            }
        }
        LayoutSnapshot::Group {
            axis,
            flexes,
            children,
        } => {
            if children.is_empty() {
                return div()
                    .size_full()
                    .bg(theme.colors().surface_background)
                    .into_any_element();
            }
            let weights = normalize_flexes(flexes.as_deref(), children.len());
            let mut container = match axis {
                SnapshotAxis::Horizontal => h_flex().size_full(),
                SnapshotAxis::Vertical => v_flex().size_full(),
            };
            for (idx, child) in children.iter().enumerate() {
                let weight = weights.get(idx).copied().unwrap_or(1.0).max(0.01);
                container = container.child(
                    div()
                        .flex_grow()
                        .flex_shrink()
                        .flex_basis(gpui::relative(weight))
                        .child(render_sketch_node(child, cx)),
                );
            }
            container.into_any_element()
        }
    }
}

fn normalize_flexes(flexes: Option<&[f32]>, n: usize) -> Vec<f32> {
    match flexes {
        Some(values) if values.len() == n => {
            let sum: f32 = values.iter().copied().sum();
            if sum <= 0.0 {
                vec![1.0 / n.max(1) as f32; n]
            } else {
                values.iter().map(|v| (v / sum).max(0.0)).collect()
            }
        }
        _ => vec![1.0 / n.max(1) as f32; n],
    }
}

// TODO(phase-5/window-overview): `/` filter sub-mode — narrow the grid
// in place by fuzzy-matching window names. Deferred to keep this change
// scoped; the spec explicitly allows a follow-up.

#[cfg(test)]
mod tests {
    use super::*;
    use workspace::codon_bridge::{ItemSnapshot, PaneSnapshot};

    fn pane(kind: &str, active: bool) -> LayoutSnapshot {
        LayoutSnapshot::Pane(PaneSnapshot {
            items: vec![ItemSnapshot {
                kind: kind.to_string(),
                item_id: 1,
                active: true,
                preview: false,
            }],
            active,
            pinned_count: 0,
        })
    }

    #[test]
    fn dominant_kind_for_single_pane() {
        let layout = pane("Terminal", true);
        assert_eq!(dominant_pane_kind(Some(&layout)).as_deref(), Some("Terminal"));
    }

    #[test]
    fn dominant_kind_picks_majority() {
        let layout = LayoutSnapshot::Group {
            axis: SnapshotAxis::Horizontal,
            flexes: None,
            children: vec![pane("Terminal", false), pane("Editor", false), pane("Editor", true)],
        };
        assert_eq!(dominant_pane_kind(Some(&layout)).as_deref(), Some("Editor"));
    }

    #[test]
    fn count_panes_walks_groups() {
        let layout = LayoutSnapshot::Group {
            axis: SnapshotAxis::Vertical,
            flexes: None,
            children: vec![
                pane("Terminal", true),
                LayoutSnapshot::Group {
                    axis: SnapshotAxis::Horizontal,
                    flexes: None,
                    children: vec![pane("Editor", false), pane("Editor", false)],
                },
            ],
        };
        assert_eq!(count_panes(Some(&layout)), 3);
    }

    #[test]
    fn count_panes_empty_layout_is_one() {
        assert_eq!(count_panes(None), 1);
    }

    #[test]
    fn normalize_flexes_handles_missing() {
        let out = normalize_flexes(None, 4);
        assert_eq!(out.len(), 4);
        let sum: f32 = out.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn normalize_flexes_uses_provided_when_length_matches() {
        let out = normalize_flexes(Some(&[1.0, 3.0]), 2);
        assert!((out[0] - 0.25).abs() < 1e-5);
        assert!((out[1] - 0.75).abs() < 1e-5);
    }
}
