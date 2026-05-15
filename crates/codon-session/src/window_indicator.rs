use gpui::{
    Context, ElementId, IntoElement, ParentElement, Render, StatefulInteractiveElement as _,
    WeakEntity, Window,
};
use ui::{
    Label, LabelCommon, LabelSize, Tab, TabBar, TabPosition, Toggleable as _, h_flex,
};
use workspace::{ItemHandle, StatusItemView, Workspace, notifications::NotifyTaskExt as _};

use crate::{
    actions::{WindowGoto, persist_async},
    registry::SessionRegistry,
    runtime::{WindowRuntime, WindowRuntimeCache},
    session::WindowId,
    swap,
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
    persist_async(cx);

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

