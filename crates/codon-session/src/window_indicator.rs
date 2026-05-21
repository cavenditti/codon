use gpui::{
    Context, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, Render, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
    div, hsla, px,
};
use ui::{
    ActiveTheme as _, Color, FluentBuilder as _, Label, LabelCommon, LabelSize,
    h_flex,
};
use workspace::{
    ItemHandle, StatusItemView, Workspace, codon_bridge::PaneSnapshot,
    notifications::NotifyTaskExt as _,
};

use crate::{
    actions::{WindowGoto, persist_debounced},
    registry::SessionRegistry,
    runtime::{WindowRuntime, WindowRuntimeCache},
    session::{SessionId, Window as SessionWindow, WindowId},
    swap::{self, LayoutSnapshot},
};

/// Windows-in-session indicator, rendered in the center of the status bar
/// using a tab-bar-shaped strip without close buttons.
pub struct WindowsStatusItem {
    workspace: WeakEntity<Workspace>,
}

impl WindowsStatusItem {
    pub fn new(workspace: WeakEntity<Workspace>) -> Self {
        Self { workspace }
    }
}

impl Render for WindowsStatusItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let registry = SessionRegistry::global(cx);
        let Some(session) = registry.active() else {
            return h_flex();
        };
        if session.windows.is_empty() {
            return h_flex();
        }
        let active = session.active_window;
        // Windows always exist conceptually (one slot per digit binding);
        // the indicator only shows the slots that hold user content,
        // plus the active slot so the user can always see where they
        // are. This keeps a brand-new session showing a single tab and
        // a half-used session showing only the used + active tabs.
        let displayed = session.displayed_window_indices();

        let live_active_kind = self
            .workspace
            .upgrade()
            .and_then(|ws| active_workspace_item_kind(&ws.read(cx), cx));

        let workspace = self.workspace.clone();
        let mut bar = h_flex().gap(px(2.));
        for &idx in &displayed {
            let win = &session.windows[idx];
            let kind = if idx == active {
                live_active_kind.as_deref()
            } else {
                win.layout.as_ref().and_then(active_item_kind_in_snapshot)
            };
            let tail = tab_tail_for(win, idx, kind);
            let base = base_color_for_kind(kind, cx);
            let is_active = idx == active;
            let attention = !is_active && win.needs_attention;
            let chip = window_chip(idx, &win.id, is_active, attention, base, tail, cx, {
                let workspace = workspace.clone();
                let target = win.id;
                move |window, cx| {
                    if let Some(ws) = workspace.upgrade() {
                        ws.update(cx, |ws, cx| {
                            switch_to_window(ws, target, window, cx);
                        });
                    }
                }
            });
            bar = bar.child(chip);
        }
        bar
    }
}

/// Two-segment "chip" replacing the standard `Tab`: a slightly more
/// intense numeral on the left, a subtler kind/name tail on the right.
/// Splitting the surface lets the eye lock onto the jump digit without
/// reading the rest of the chip — the user told us the dot+label combo
/// looked dot-heavy and the digit was hard to scan against a wide tail.
///
/// Sizing note: the chip is built without an outer margin and without a
/// `TabBar` wrapper. Wrapping in `TabBar` previously forced the row to
/// `Tab::container_height` (~24 px), which then stacked on top of the
/// chip's own `my(2)` + border to overflow the status bar's natural
/// ~22 px height. We now hand-roll an `h_flex` row so the chips inherit
/// the bar's content height instead of dictating their own.
fn window_chip<F>(
    idx: usize,
    win_id: &WindowId,
    is_active: bool,
    needs_attention: bool,
    base: Hsla,
    tail: Option<String>,
    cx: &Context<WindowsStatusItem>,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&mut Window, &mut Context<WindowsStatusItem>) + 'static,
{
    let id: ElementId = ElementId::Name(format!("codon-window-{}", win_id.0).into());
    let intense_bg = base.opacity(if is_active {
        0.85
    } else if needs_attention {
        0.65
    } else {
        0.45
    });
    let subtle_bg = base.opacity(if is_active {
        0.30
    } else if needs_attention {
        0.24
    } else {
        0.16
    });
    let number_label = (idx + 1).to_string();

    // Dark-grey numeral on the bright active background so the jump
    // digit stays readable against the most saturated chip; inactive
    // chips keep the theme's muted text so they recede.
    let number_color = if is_active {
        Color::Custom(hsla(0.0, 0.0, 0.12, 1.0))
    } else {
        Color::Muted
    };

    let number_segment = div()
        .h_full()
        .flex()
        .items_center()
        .px(px(5.))
        .bg(intense_bg)
        .child(
            Label::new(number_label)
                .size(LabelSize::Small)
                .weight(FontWeight::BOLD)
                .color(number_color),
        );

    let tail_segment = tail.map(|text| {
        div()
            .h_full()
            .flex()
            .items_center()
            .px(px(5.))
            .bg(subtle_bg)
            .child(
                Label::new(text)
                    .size(LabelSize::Small)
                    .color(Color::Default),
            )
    });

    // Active wins over attention: the user is currently *in* the window
    // they would otherwise want to be notified about. Attention is the
    // fallback signal for the *other* chips that have unseen output.
    let border = if is_active {
        hsla(0.0, 0.0, 1.0, 1.0)
    } else if needs_attention {
        cx.theme().status().warning
    } else {
        cx.theme().colors().border.opacity(0.0)
    };

    div()
        .id(id)
        .flex()
        .items_center()
        .h(px(18.))
        .rounded_sm()
        .overflow_hidden()
        .border_1()
        .border_color(border)
        .gap(px(1.))
        .cursor_pointer()
        .child(number_segment)
        .when_some(tail_segment, |this, seg| this.child(seg))
        .on_click(cx.listener(move |_, _event, window, cx| on_click(window, cx)))
}

/// Map an item-kind string (as produced by `serialized_item_kind` or the
/// codon pane-kind registry) to the short tail shown in the tab label.
/// Returns `None` for kinds we don't recognise — callers then fall back
/// to no tail at all (the numeral alone), preserving the "name unset"
/// signal without inventing a misleading label.
fn label_for_kind(kind: Option<&str>) -> Option<&'static str> {
    match kind? {
        "Terminal" => Some("term"),
        "FileManager" => Some("FM"),
        "GitPanel" => Some("git"),
        "AgentPanel" => Some("agent"),
        "Outline Panel" => Some("outline"),
        "DebugPanel" => Some("debug"),
        "Editor" => Some("edit"),
        _ => None,
    }
}

/// Resolve a kind to a theme-aware base hue. The chip then renders that
/// hue at a more intense alpha behind the numeral and a subtler alpha
/// behind the tail — see [`window_chip`]. Unrecognised / missing kinds
/// fall back to the theme's element background so the chip still shows
/// the jump number even when we can't characterise the pane.
fn base_color_for_kind(kind: Option<&str>, cx: &gpui::App) -> Hsla {
    let theme = cx.theme();
    match kind {
        Some("Terminal") => theme.status().success,
        Some("FileManager") => theme.status().info,
        Some("GitPanel") => theme.status().warning,
        Some("AgentPanel") => theme.colors().text_accent,
        Some("Outline Panel") => theme.colors().text_muted,
        Some("DebugPanel") => theme.status().error,
        Some("Editor") => theme.colors().text,
        _ => theme.colors().element_background,
    }
}

/// `true` when `name` matches one of the two auto-generated forms
/// applied by [`Session::add_window`]: the position-based fallback
/// (`"{idx+1}"`) and the id-based fallback (`"{id.0}"`). Both signal
/// "the user has not renamed this window," so the kind-derived tail
/// should take over.
fn name_is_autogenerated(name: &str, idx: usize, id: WindowId) -> bool {
    name == (idx + 1).to_string() || name == id.0.to_string()
}

fn tab_tail_for(win: &SessionWindow, idx: usize, kind: Option<&str>) -> Option<String> {
    if !name_is_autogenerated(&win.name, idx, win.id) {
        return Some(win.name.clone());
    }
    label_for_kind(kind).map(|s| s.to_string())
}

/// Inspect the workspace's currently focused pane and return the
/// canonical kind string of its active item — the same string the
/// item registers under `serialized_item_kind`, or its
/// `tab_content_text` for adapter-hosted panels that don't implement
/// `SerializableItem`. The window-indicator uses the result both to
/// pick a tail label and to color the dot.
fn active_workspace_item_kind(workspace: &Workspace, cx: &gpui::App) -> Option<String> {
    let pane = workspace.active_pane().read(cx);
    let item = pane.active_item()?;
    if let Some(serializable) = item.to_serializable_item_handle(cx) {
        return Some(serializable.serialized_item_kind().to_string());
    }
    let text = item.tab_content_text(0, cx);
    (!text.is_empty()).then(|| text.to_string())
}

/// Walk a `LayoutSnapshot` looking for the pane that was marked
/// `active: true` at capture time, and return that pane's active
/// item's `kind`. This is the inactive-window analogue of
/// [`active_workspace_item_kind`] — for windows the user isn't
/// currently viewing, the persisted snapshot is the only handle we
/// have on "what's in there."
fn active_item_kind_in_snapshot(snapshot: &LayoutSnapshot) -> Option<&str> {
    fn find_active_pane(snapshot: &LayoutSnapshot) -> Option<&PaneSnapshot> {
        match snapshot {
            LayoutSnapshot::Group { children, .. } => children.iter().find_map(find_active_pane),
            LayoutSnapshot::Stack { members, .. } => members.iter().find_map(find_active_pane),
            LayoutSnapshot::Pane(pane) if pane.active => Some(pane),
            _ => None,
        }
    }
    fn any_pane(snapshot: &LayoutSnapshot) -> Option<&PaneSnapshot> {
        match snapshot {
            LayoutSnapshot::Group { children, .. } => children.iter().find_map(any_pane),
            LayoutSnapshot::Stack { members, .. } => members.iter().find_map(any_pane),
            LayoutSnapshot::Pane(pane) => Some(pane),
        }
    }
    let pane = find_active_pane(snapshot).or_else(|| any_pane(snapshot))?;
    let active_item = pane
        .items
        .iter()
        .find(|item| item.active)
        .or_else(|| pane.items.first())?;
    Some(active_item.kind.as_str())
}

impl StatusItemView for WindowsStatusItem {
    fn set_active_pane_item(
        &mut self,
        _: Option<&dyn ItemHandle>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The active tab now mirrors the focused pane's item kind, so a
        // pane-item swap (terminal → editor, fm → git, …) must refresh
        // the label and dot.
        cx.notify();
    }
}

pub(crate) fn switch_to_window(
    workspace: &mut Workspace,
    target: WindowId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(mut session) = registry.get(active_id) else {
        return;
    };
    let Some(target_idx) = session.windows.iter().position(|w| w.id == target) else {
        return;
    };
    if target_idx == session.active_window {
        return;
    }

    let outgoing_id = session.active().map(|w| w.id);
    let snapshot = swap::capture(workspace, window, cx);
    let runtime = capture_runtime(workspace);
    if let Some(active) = session.active_mut() {
        active.layout = Some(snapshot);
    }
    if let (Some(outgoing_window_id), Some(rt)) = (outgoing_id, runtime) {
        WindowRuntimeCache::global(cx).insert(active_id, outgoing_window_id, rt);
    }

    session.set_active_window(target_idx);
    let incoming_window_id = session.windows.get(target_idx).map(|w| w.id);
    let incoming_layout = session.windows.get(target_idx).and_then(|w| w.layout.clone());
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save window switch: {err:?}");
    }
    persist_debounced(cx);

    let cache = WindowRuntimeCache::global(cx);
    let cached_runtime = incoming_window_id.and_then(|id| cache.take(active_id, id));
    if let Some(rt) = cached_runtime {
        log::debug!(
            "restoring window {:?} from in-memory runtime cache",
            incoming_window_id
        );
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
    } else if let Some(layout) = incoming_layout {
        log::debug!(
            "restoring window {:?} from persisted snapshot (no runtime cache hit)",
            incoming_window_id
        );
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
    } else {
        log::debug!(
            "no state for window {:?}; opening fresh empty pane",
            incoming_window_id
        );
        workspace.replace_center_with_empty_pane(window, cx);
    }
}

fn capture_runtime(workspace: &Workspace) -> Option<WindowRuntime> {
    let root = workspace.center().root.clone();
    let active_pane = Some(workspace.active_pane().clone());
    Some(WindowRuntime { root, active_pane })
}

/// Swap the workspace's center group to `target` *without* mutating the
/// session registry or kicking off a persistence write. Used by the
/// overview modal to live-preview the highlighted row as the user
/// navigates; the actual `session.active_window` is updated only when
/// the user commits via Enter (see [`commit_active_window`]).
///
/// The outgoing pane tree is stashed in `WindowRuntimeCache` under
/// `current_window_id` so a subsequent restore (Esc) can pull it back.
pub(crate) fn preview_switch_to_window(
    workspace: &mut Workspace,
    session_id: SessionId,
    current_window_id: WindowId,
    target_window_id: WindowId,
    target_layout: Option<LayoutSnapshot>,
    restore_focus: FocusHandle,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if target_window_id == current_window_id {
        return;
    }

    if let Some(rt) = capture_runtime(workspace) {
        WindowRuntimeCache::global(cx).insert(session_id, current_window_id, rt);
    }

    let cache = WindowRuntimeCache::global(cx);
    let cached_runtime = cache.take(session_id, target_window_id);
    if let Some(rt) = cached_runtime {
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
        window.focus(&restore_focus, cx);
    } else if let Some(layout) = target_layout {
        let apply_task = swap::apply(workspace, layout, window, cx);
        // The async snapshot path ends in `cx.focus_self(window)` on the
        // workspace, which would yank focus off the modal mid-preview.
        // Chain a refocus so the modal regains focus once the swap settles.
        cx.spawn_in(window, async move |_, cx| {
            if let Err(err) = apply_task.await {
                log::warn!("preview swap failed: {err:?}");
            }
            cx.update(|window, cx| window.focus(&restore_focus, cx)).ok();
        })
        .detach();
    } else {
        workspace.replace_center_with_empty_pane(window, cx);
        window.focus(&restore_focus, cx);
    }
}

/// Record `target_idx` as the session's active window in the registry
/// and persist the change. Assumes the workspace already *shows* that
/// window — the overview modal calls this after a preview to commit
/// the user's Enter selection without doing a redundant swap.
pub(crate) fn commit_active_window(target_idx: usize, cx: &gpui::App) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(mut session) = registry.get(active_id) else {
        return;
    };
    if target_idx >= session.windows.len() || target_idx == session.active_window {
        return;
    }
    session.set_active_window(target_idx);
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not commit active window after preview: {err:?}");
    }
    persist_debounced(cx);
}

/// Wire WindowGoto(usize) action handler that switches to window at index.
pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_window_goto);
}

fn handle_window_goto(
    workspace: &mut Workspace,
    action: &WindowGoto,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(session) = registry.get(active_id) else {
        return;
    };
    let Some(target) = session.windows.get(action.0).map(|w| w.id) else {
        return;
    };
    switch_to_window(workspace, target, window, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use workspace::codon_bridge::{ItemSnapshot, SnapshotAxis};

    fn pane(active: bool, items: Vec<ItemSnapshot>) -> LayoutSnapshot {
        LayoutSnapshot::Pane(PaneSnapshot {
            items,
            active,
            pinned_count: 0,
        })
    }

    fn item(kind: &str, active: bool) -> ItemSnapshot {
        ItemSnapshot {
            kind: kind.into(),
            item_id: 1,
            active,
            preview: false,
        }
    }

    #[test]
    fn auto_name_detected_for_both_index_and_id_forms() {
        let win = SessionWindow::new(WindowId(7), "7");
        assert!(name_is_autogenerated(&win.name, 2, win.id));
        let win = SessionWindow::new(WindowId(7), "3");
        assert!(name_is_autogenerated(&win.name, 2, win.id));
        let win = SessionWindow::new(WindowId(7), "scratch");
        assert!(!name_is_autogenerated(&win.name, 2, win.id));
    }

    #[test]
    fn tail_prefers_user_name_over_kind() {
        let win = SessionWindow::new(WindowId(2), "build");
        let tail = tab_tail_for(&win, 1, Some("Terminal"));
        assert_eq!(tail.as_deref(), Some("build"));
    }

    #[test]
    fn tail_falls_through_to_kind_for_autogenerated_name() {
        let win = SessionWindow::new(WindowId(2), "2");
        let tail = tab_tail_for(&win, 1, Some("FileManager"));
        assert_eq!(tail.as_deref(), Some("FM"));
        let tail = tab_tail_for(&win, 1, Some("Terminal"));
        assert_eq!(tail.as_deref(), Some("term"));
        let tail = tab_tail_for(&win, 1, None);
        assert!(tail.is_none());
    }

    #[test]
    fn label_for_kind_recognises_known_panes() {
        assert_eq!(label_for_kind(Some("Terminal")), Some("term"));
        assert_eq!(label_for_kind(Some("GitPanel")), Some("git"));
        assert_eq!(label_for_kind(Some("AgentPanel")), Some("agent"));
        assert_eq!(label_for_kind(Some("DebugPanel")), Some("debug"));
        assert_eq!(label_for_kind(Some("Outline Panel")), Some("outline"));
        assert_eq!(label_for_kind(Some("Editor")), Some("edit"));
        assert_eq!(label_for_kind(Some("Mystery")), None);
        assert_eq!(label_for_kind(None), None);
    }

    #[test]
    fn snapshot_walker_picks_active_pane_in_nested_group() {
        let snap = LayoutSnapshot::Group {
            axis: SnapshotAxis::Horizontal,
            flexes: None,
            children: vec![
                pane(false, vec![item("Editor", true)]),
                LayoutSnapshot::Stack {
                    members: vec![
                        pane(false, vec![item("FileManager", true)]),
                        pane(true, vec![item("Terminal", true)]),
                    ],
                    active: 1,
                },
            ],
        };
        assert_eq!(active_item_kind_in_snapshot(&snap), Some("Terminal"));
    }

    #[test]
    fn snapshot_walker_falls_back_to_any_pane_when_none_marked_active() {
        let snap = LayoutSnapshot::Group {
            axis: SnapshotAxis::Vertical,
            flexes: None,
            children: vec![pane(false, vec![item("Editor", true)])],
        };
        assert_eq!(active_item_kind_in_snapshot(&snap), Some("Editor"));
    }

    #[test]
    fn snapshot_walker_returns_none_for_empty_pane() {
        let snap = pane(true, vec![]);
        assert_eq!(active_item_kind_in_snapshot(&snap), None);
    }
}
