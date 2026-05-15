//! Codon-side dispatch for the debug panel.

use debugger_ui::debugger_panel::DebugPanel;
use gpui::{AppContext as _, Context, Entity, Focusable as _, WeakEntity, Window};
use workspace::Workspace;

use crate::actions::{OpenDebug, PeekDebug};
use crate::adapter::PanelItemAdapter;
use crate::peek::{PeekSide, peek_panel};
use crate::registry::focus_existing_adapter;

pub(crate) fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_open_debug);
    workspace.register_action(handle_peek_debug);
}

fn handle_open_debug(
    workspace: &mut Workspace,
    _: &OpenDebug,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if focus_existing_adapter::<DebugPanel>(workspace, window, cx) {
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match DebugPanel::load(weak.clone(), cx).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load DebugPanel: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            mount_debug_pane(workspace, panel, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while mounting DebugPanel: {err:#}");
        }
    })
    .detach();
}

fn handle_peek_debug(
    workspace: &mut Workspace,
    _: &PeekDebug,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(existing) = workspace.panel::<DebugPanel>(cx) {
        peek_panel(workspace, existing, PeekSide::Bottom, window, cx);
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match DebugPanel::load(weak.clone(), cx).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load DebugPanel for peek: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            peek_panel(workspace, panel, PeekSide::Bottom, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while peeking DebugPanel: {err:#}");
        }
    })
    .detach();
}

fn mount_debug_pane(
    workspace: &mut Workspace,
    panel: Entity<DebugPanel>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_pane = workspace.active_pane().clone();
    crate::registry::ensure_pane_tab_bar(&active_pane, cx);
    let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
    workspace.add_item_to_active_pane(Box::new(adapter), None, true, window, cx);
    panel.focus_handle(cx).focus(window, cx);
}
