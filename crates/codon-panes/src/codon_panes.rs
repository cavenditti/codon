//! codon-panes — host every Zed `Panel` impl as a workspace `Item` via
//! `PanelItemAdapter<P>`, with an opt-in transient `peek` placement that
//! re-uses Zed's dock infrastructure for on-demand sidebar viewing.
//!
//! See `.specs/codon/panes-from-panels.spec.md` and the tasks under
//! `.specs/phase-12/` for the design contract.

pub mod actions;
pub mod adapter;
pub mod agent;
pub mod debug;
pub mod git;
pub mod outline;
pub mod peek;
mod registry;

pub use actions::{
    OpenAgent, OpenDebug, OpenGit, OpenOutline, PeekAgent, PeekDebug, PeekDismiss, PeekGit,
    PeekOutline,
};
pub use adapter::{PanelItemAdapter, PanelItemEvent};
pub use peek::{PeekSide, peek_panel};

use agent_ui::AgentPanel;
use debugger_ui::debugger_panel::DebugPanel;
use git_ui::git_panel::GitPanel;
use gpui::App;
use outline_panel::OutlinePanel;
use workspace::{
    Workspace,
    codon_bridge::{register_item_panel_kind_fn, register_panel_restorer},
    dock::Panel,
    item::ItemHandle,
};

/// Wire codon-panes into the workspace lifecycle.
///
/// Registers (a) a codon-bridge hook letting `capture_layout` recognise
/// adapter-hosted panels by their `persistent_name`, and (b) one
/// panel-restorer per converted panel so `apply_layout` can rehydrate
/// them after a window switch or restart.
pub fn init(_cx: &mut App) {
    register_item_panel_kind_fn(detect_panel_kind);
    register_panel_restorer(AgentPanel::persistent_name(), restorer::restore_agent);
    register_panel_restorer(GitPanel::persistent_name(), restorer::restore_git);
    register_panel_restorer(OutlinePanel::persistent_name(), restorer::restore_outline);
    register_panel_restorer(DebugPanel::persistent_name(), restorer::restore_debug);
}

fn detect_panel_kind(handle: &dyn ItemHandle) -> Option<&'static str> {
    let entity_type = handle.to_any_view().entity_type();
    if entity_type == std::any::TypeId::of::<PanelItemAdapter<AgentPanel>>() {
        Some(AgentPanel::persistent_name())
    } else if entity_type == std::any::TypeId::of::<PanelItemAdapter<GitPanel>>() {
        Some(GitPanel::persistent_name())
    } else if entity_type == std::any::TypeId::of::<PanelItemAdapter<OutlinePanel>>() {
        Some(OutlinePanel::persistent_name())
    } else if entity_type == std::any::TypeId::of::<PanelItemAdapter<DebugPanel>>() {
        Some(DebugPanel::persistent_name())
    } else {
        None
    }
}

mod restorer {
    use agent_ui::AgentPanel;
    use debugger_ui::debugger_panel::DebugPanel;
    use git_ui::git_panel::GitPanel;
    use gpui::{AppContext as _, AsyncWindowContext, Task, WeakEntity};
    use outline_panel::OutlinePanel;
    use workspace::{Workspace, item::ItemHandle};

    use crate::adapter::PanelItemAdapter;

    pub(super) fn restore_agent(
        workspace: WeakEntity<Workspace>,
        cx: AsyncWindowContext,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        cx.spawn(async move |cx| {
            let panel = AgentPanel::load(workspace.clone(), cx.clone()).await?;
            workspace.update_in(cx, |_workspace, window, cx| -> anyhow::Result<_> {
                let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
                Ok(Box::new(adapter) as Box<dyn ItemHandle>)
            })?
        })
    }

    pub(super) fn restore_git(
        workspace: WeakEntity<Workspace>,
        cx: AsyncWindowContext,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        cx.spawn(async move |cx| {
            let panel = GitPanel::load(workspace.clone(), cx.clone()).await?;
            workspace.update_in(cx, |_workspace, window, cx| -> anyhow::Result<_> {
                let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
                Ok(Box::new(adapter) as Box<dyn ItemHandle>)
            })?
        })
    }

    pub(super) fn restore_outline(
        workspace: WeakEntity<Workspace>,
        cx: AsyncWindowContext,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        cx.spawn(async move |cx| {
            let panel = OutlinePanel::load(workspace.clone(), cx.clone()).await?;
            workspace.update_in(cx, |_workspace, window, cx| -> anyhow::Result<_> {
                let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
                Ok(Box::new(adapter) as Box<dyn ItemHandle>)
            })?
        })
    }

    pub(super) fn restore_debug(
        workspace: WeakEntity<Workspace>,
        cx: AsyncWindowContext,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        cx.spawn(async move |cx| {
            let panel = DebugPanel::load(workspace.clone(), cx).await?;
            workspace.update_in(cx, |_workspace, window, cx| -> anyhow::Result<_> {
                let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
                Ok(Box::new(adapter) as Box<dyn ItemHandle>)
            })?
        })
    }
}

/// Per-workspace action wiring. Mirrors the codon-agent /
/// codon-session pattern (`register_for_workspace(workspace: &mut Workspace)`).
pub fn register_for_workspace(workspace: &mut Workspace) {
    agent::register_for_workspace(workspace);
    git::register_for_workspace(workspace);
    outline::register_for_workspace(workspace);
    debug::register_for_workspace(workspace);
    workspace.register_action(peek::handle_peek_dismiss);
}
