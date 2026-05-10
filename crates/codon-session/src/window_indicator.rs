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
                use std::cmp::Ordering;
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

fn switch_to_window(
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
    let snapshot = swap::capture(workspace, window, cx);
    if let Some(active) = session.active_mut() {
        active.layout = Some(snapshot);
    }
    session.active_window = target_idx;
    let target_layout = session
        .windows
        .get(target_idx)
        .and_then(|w| w.layout.clone())
        .unwrap_or_else(workspace::codon_bridge::LayoutSnapshot::empty_pane);
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save window switch: {err:?}");
    }
    persist_async(cx);
    let weak = workspace.weak_handle();
    swap::apply(workspace, target_layout, window, cx).detach_and_notify_err(weak, window, cx);
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

