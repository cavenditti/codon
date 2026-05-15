//! Codon-side dispatch for the outline panel.

use gpui::{AppContext as _, Context, Entity, Focusable as _, WeakEntity, Window};
use outline_panel::OutlinePanel;
use workspace::Workspace;

use crate::actions::{OpenOutline, PeekOutline};
use crate::adapter::PanelItemAdapter;
use crate::peek::{PeekSide, peek_panel};
use crate::registry::focus_existing_adapter;

pub(crate) fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_open_outline);
    workspace.register_action(handle_peek_outline);
}

fn handle_open_outline(
    workspace: &mut Workspace,
    _: &OpenOutline,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if focus_existing_adapter::<OutlinePanel>(workspace, window, cx) {
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match OutlinePanel::load(weak.clone(), cx.clone()).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load OutlinePanel: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            mount_outline_pane(workspace, panel, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while mounting OutlinePanel: {err:#}");
        }
    })
    .detach();
}

fn handle_peek_outline(
    workspace: &mut Workspace,
    _: &PeekOutline,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(existing) = workspace.panel::<OutlinePanel>(cx) {
        peek_panel(workspace, existing, PeekSide::Left, window, cx);
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match OutlinePanel::load(weak.clone(), cx.clone()).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load OutlinePanel for peek: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            peek_panel(workspace, panel, PeekSide::Left, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while peeking OutlinePanel: {err:#}");
        }
    })
    .detach();
}

fn mount_outline_pane(
    workspace: &mut Workspace,
    panel: Entity<OutlinePanel>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_pane = workspace.active_pane().clone();
    crate::registry::ensure_pane_tab_bar(&active_pane, cx);
    let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
    workspace.add_item_to_active_pane(Box::new(adapter), None, true, window, cx);
    panel.focus_handle(cx).focus(window, cx);
}
