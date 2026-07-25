//! Codon-side dispatch for the git panel. Two actions:
//!
//! - `OpenGit` mounts the panel as a workspace pane.
//! - `PeekGit` opens it via `peek_panel(PeekSide::Left)`.
//!
//! The existing `[bindings.git_panel.*]` keymap blocks and the
//! mode-tracker integration (see `TASK:phase-4/git-panel-modal-integration`)
//! keep working unchanged — they fire whether the panel is hosted by the
//! adapter or by the peek dock.

use git_ui::git_panel::GitPanel;
use gpui::{AppContext as _, Context, Entity, Focusable as _, WeakEntity, Window};
use workspace::Workspace;

use crate::actions::{OpenGit, PeekGit};
use crate::adapter::PanelItemAdapter;
use crate::peek::{PeekSide, peek_panel};
use crate::registry::{ensure_pane_tab_bar, focus_existing_adapter};

pub(crate) fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_open_git);
    workspace.register_action(handle_peek_git);
}

fn handle_open_git(
    workspace: &mut Workspace,
    _: &OpenGit,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if focus_existing_adapter::<GitPanel>(workspace, window, cx) {
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match GitPanel::load(weak.clone(), cx.clone()).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load GitPanel: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            mount_git_pane(workspace, panel, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while mounting GitPanel: {err:#}");
        }
    })
    .detach();
}

fn handle_peek_git(
    workspace: &mut Workspace,
    _: &PeekGit,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(existing) = workspace.panel::<GitPanel>(cx) {
        peek_panel(workspace, existing, PeekSide::Left, window, cx);
        return;
    }
    let weak: WeakEntity<Workspace> = workspace.weak_handle();
    cx.spawn_in(window, async move |_workspace, cx| {
        let panel = match GitPanel::load(weak.clone(), cx.clone()).await {
            Ok(panel) => panel,
            Err(err) => {
                log::warn!("codon-panes: failed to load GitPanel for peek: {err:#}");
                return;
            }
        };
        if let Err(err) = weak.update_in(cx, |workspace, window, cx| {
            peek_panel(workspace, panel, PeekSide::Left, window, cx);
        }) {
            log::warn!("codon-panes: workspace gone while peeking GitPanel: {err:#}");
        }
    })
    .detach();
}

fn mount_git_pane(
    workspace: &mut Workspace,
    panel: Entity<GitPanel>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_pane = workspace.active_pane().clone();
    ensure_pane_tab_bar(&active_pane, cx);
    let adapter = cx.new(|cx| PanelItemAdapter::new(panel.clone(), window, cx));
    workspace.add_item_to_active_pane(Box::new(adapter), None, true, window, cx);
    panel.focus_handle(cx).focus(window, cx);
}
