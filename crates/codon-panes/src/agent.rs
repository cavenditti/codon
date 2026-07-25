//! Codon-side dispatch for the agent panel. Two actions:
//!
//! - `OpenAgent` mounts the panel as a regular workspace pane via
//!   `PanelItemAdapter`, focusing the existing tab if one exists.
//! - `PeekAgent` opens the panel via `peek_panel(PeekSide::Right)`.
//!
//! Both routes preserve the cross-pane `seed_explain_with_selection` flow
//! from `codon-agent` — those verbs continue to call into `AgentPanel`,
//! and adapter-hosted tabs share the same singleton entity.

use agent_ui::AgentPanel;
use gpui::{AppContext as _, Context, Entity, Focusable as _, WeakEntity, Window};
use workspace::Workspace;

use crate::actions::{OpenAgent, PeekAgent};
use crate::adapter::PanelItemAdapter;
use crate::peek::{PeekSide, peek_panel};
use crate::registry::{ensure_pane_tab_bar, focus_existing_adapter};

pub(crate) fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_open_agent);
    workspace.register_action(handle_peek_agent);
}

fn handle_open_agent(
    workspace: &mut Workspace,
    _: &OpenAgent,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if focus_existing_adapter::<AgentPanel>(workspace, window, cx) {
        return;
    }
    let weak = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match AgentPanel::load(weak.clone(), cx.clone()).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load AgentPanel: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            mount_agent_pane(workspace, panel, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while mounting AgentPanel: {err:#}");
        }
    })
    .detach();
}

fn handle_peek_agent(
    workspace: &mut Workspace,
    _: &PeekAgent,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(existing) = workspace.panel::<AgentPanel>(cx) {
        peek_panel(workspace, existing, PeekSide::Right, window, cx);
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match AgentPanel::load(weak.clone(), cx.clone()).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load AgentPanel for peek: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            peek_panel(workspace, panel, PeekSide::Right, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while peeking AgentPanel: {err:#}");
        }
    })
    .detach();
}

fn mount_agent_pane(
    workspace: &mut Workspace,
    panel: Entity<AgentPanel>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_pane = workspace.active_pane().clone();
    ensure_pane_tab_bar(&active_pane, cx);
    let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
    workspace.add_item_to_active_pane(Box::new(adapter), None, true, window, cx);
    panel.focus_handle(cx).focus(window, cx);
}
