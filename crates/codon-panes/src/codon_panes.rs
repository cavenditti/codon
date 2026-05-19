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
    OpenAgent, OpenDebug, OpenFromRegister, OpenGit, OpenOutline, PeekAgent, PeekDebug,
    PeekDismiss, PeekGit, PeekOutline,
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
    workspace.register_action(handle_open_from_register);
}

/// `codon_panes::OpenFromRegister` — read the armed register, expect a
/// `Selection::Files` payload, open each path in the workspace.
///
/// This is the proof point for the `"<char>` *read* side of the
/// register prefix: the action handler pulls the pending name out of
/// the [`codon_session::RegisterStore`], reads the named register, and
/// dispatches a normal "open path" verb per file. Non-`Files`
/// selections + unarmed prefixes are debug-logged no-ops so a
/// keystroke without a primed register can't crash.
fn handle_open_from_register(
    workspace: &mut Workspace,
    _: &OpenFromRegister,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let store = codon_session::RegisterStore::global(cx);
    let Some(pending) = store.take_pending() else {
        log::debug!("codon-panes: OpenFromRegister without armed register — noop");
        return;
    };
    let Some(selection) = store.read(pending.name) else {
        log::debug!(
            "codon-panes: OpenFromRegister register '{}' is empty",
            pending.name
        );
        return;
    };
    let codon_mode::Selection::Files(paths) = selection else {
        log::debug!(
            "codon-panes: OpenFromRegister register '{}' does not hold files \
             (helix-text-register interop is the follow-up task)",
            pending.name
        );
        return;
    };
    for path in paths {
        // Reuse Zed's `workspace::OpenVisible` path-open verb. Each path
        // becomes its own dispatch so a multi-file register prompts the
        // workspace's regular conflict-resolution UI for each.
        workspace
            .open_abs_path(
                path.clone(),
                workspace::OpenOptions {
                    visible: Some(workspace::OpenVisible::All),
                    ..Default::default()
                },
                window,
                cx,
            )
            .detach_and_log_err(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    /// The adapter-detection predicates are simple `TypeId::of` checks
    /// against `PanelItemAdapter<P>` for four distinct `P`. The
    /// load-bearing invariant — and the only piece of pure logic worth
    /// pinning — is that those four `TypeId`s are pairwise distinct, so
    /// `is_agent_adapter` never silently agrees that a `GitPanel`
    /// adapter is the agent panel.
    #[test]
    fn adapter_type_ids_are_pairwise_distinct() {
        let ids = [
            TypeId::of::<PanelItemAdapter<AgentPanel>>(),
            TypeId::of::<PanelItemAdapter<GitPanel>>(),
            TypeId::of::<PanelItemAdapter<OutlinePanel>>(),
            TypeId::of::<PanelItemAdapter<DebugPanel>>(),
        ];
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(
                    ids[i], ids[j],
                    "adapter TypeIds at indices {i} and {j} collide"
                );
            }
        }
    }

    /// The `persistent_name()` of each wrapped panel identifies the
    /// kind for registration with `codon_register_pane_kind`. Two
    /// panels sharing a name would clobber each other in the global
    /// registry, so pin that they differ.
    #[test]
    fn persistent_names_are_pairwise_distinct() {
        let names = [
            AgentPanel::persistent_name(),
            GitPanel::persistent_name(),
            OutlinePanel::persistent_name(),
            DebugPanel::persistent_name(),
        ];
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(
                    names[i], names[j],
                    "persistent_name collision between adapter kinds at indices {i} and {j}"
                );
            }
        }
    }
}
