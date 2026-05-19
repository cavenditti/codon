use gpui::{
    Context, ElementId, FocusHandle, IntoElement, ParentElement, Render,
    StatefulInteractiveElement as _, WeakEntity, Window,
};
use ui::{
    Label, LabelCommon, LabelSize, Tab, TabBar, TabPosition, Toggleable as _, h_flex,
};
use workspace::{ItemHandle, StatusItemView, Workspace, notifications::NotifyTaskExt as _};

use crate::{
    actions::{WindowGoto, persist_debounced},
    registry::SessionRegistry,
    runtime::{WindowRuntime, WindowRuntimeCache},
    session::{SessionId, WindowId},
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
        let total = session.windows.len();

        let workspace = self.workspace.clone();
        let mut bar = TabBar::new("codon-windows-indicator");
        for (idx, win) in session.windows.iter().enumerate() {
            let id: ElementId = ElementId::Name(format!("codon-window-{}", win.id.0).into());
            let position = if idx == 0 {
                TabPosition::First
            } else if idx + 1 == total {
                TabPosition::Last
            } else {
                let cmp = idx.cmp(&active);
                TabPosition::Middle(cmp)
            };
            let label = win.name.clone();
            let target = win.id;
            let workspace = workspace.clone();
            let tab = Tab::new(id)
                .position(position)
                .toggle_state(idx == active)
                .child(Label::new(label).size(LabelSize::Small))
                .on_click(cx.listener(move |_, _click, window, cx| {
                    if let Some(ws) = workspace.upgrade() {
                        ws.update(cx, |ws, cx| {
                            switch_to_window(ws, target, window, cx);
                        });
                    }
                }));
            bar = bar.child(tab);
        }
        h_flex().child(bar)
    }
}

impl StatusItemView for WindowsStatusItem {
    fn set_active_pane_item(
        &mut self,
        _: Option<&dyn ItemHandle>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
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

