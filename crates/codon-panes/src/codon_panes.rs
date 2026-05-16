//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.
//!
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
    codon_bridge::{CodonPaneKindSpec, codon_register_pane_kind},
    dock::Panel,
    item::ItemHandle,
};

/// Wire codon-panes into the workspace lifecycle.
///
/// Registers one [`CodonPaneKindSpec`] per converted panel. The spec
/// bundles the adapter-detection predicate (used by `capture_layout` to
/// recognise adapter-hosted panels) with the async factory (used by
/// `apply_layout` to rehydrate them after a window switch or restart).
pub fn init(_cx: &mut App) {
    codon_register_pane_kind(CodonPaneKindSpec {
        kind: AgentPanel::persistent_name(),
        matches: is_agent_adapter,
        restore: restorer::restore_agent,
    });
    codon_register_pane_kind(CodonPaneKindSpec {
        kind: GitPanel::persistent_name(),
        matches: is_git_adapter,
        restore: restorer::restore_git,
    });
    codon_register_pane_kind(CodonPaneKindSpec {
        kind: OutlinePanel::persistent_name(),
        matches: is_outline_adapter,
        restore: restorer::restore_outline,
    });
    codon_register_pane_kind(CodonPaneKindSpec {
        kind: DebugPanel::persistent_name(),
        matches: is_debug_adapter,
        restore: restorer::restore_debug,
    });
}

fn is_agent_adapter(handle: &dyn ItemHandle) -> bool {
    handle.to_any_view().entity_type() == std::any::TypeId::of::<PanelItemAdapter<AgentPanel>>()
}

fn is_git_adapter(handle: &dyn ItemHandle) -> bool {
    handle.to_any_view().entity_type() == std::any::TypeId::of::<PanelItemAdapter<GitPanel>>()
}

fn is_outline_adapter(handle: &dyn ItemHandle) -> bool {
    handle.to_any_view().entity_type() == std::any::TypeId::of::<PanelItemAdapter<OutlinePanel>>()
}

fn is_debug_adapter(handle: &dyn ItemHandle) -> bool {
    handle.to_any_view().entity_type() == std::any::TypeId::of::<PanelItemAdapter<DebugPanel>>()
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
