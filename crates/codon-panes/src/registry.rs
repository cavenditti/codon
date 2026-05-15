//! Helpers shared across per-panel dispatch modules.

use gpui::{Context, Entity, Focusable as _, Window};
use workspace::{Pane, Workspace, dock::Panel};

use crate::adapter::PanelItemAdapter;

/// Force the tab bar on for a pane that's about to host an adapter. The
/// global `TabBarSettings.show` setting can be `false` in a user's Zed
/// config, which would otherwise hide the new panel tab and make
/// `OpenAgent` look like it replaced the previous item.
pub(crate) fn ensure_pane_tab_bar(pane: &Entity<Pane>, cx: &mut Context<Workspace>) {
    pane.update(cx, |pane, _cx| {
        pane.set_should_display_tab_bar(|_, _| true);
    });
}

/// Walk every pane in the workspace and focus the existing
/// `PanelItemAdapter<P>` tab if one is mounted. Returns `true` when an
/// existing adapter was focused (caller should short-circuit the load
/// flow); `false` otherwise.
pub(crate) fn focus_existing_adapter<P: Panel>(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> bool {
    let target = workspace
        .panes()
        .iter()
        .cloned()
        .find_map(|pane| {
            pane.read(cx)
                .items()
                .find(|item| item.downcast::<PanelItemAdapter<P>>().is_some())
                .map(|item| (pane.clone(), item.item_id()))
        });
    if let Some((pane, item_id)) = target {
        pane.update(cx, |pane, cx| {
            let active_index = pane
                .items()
                .position(|item| item.item_id() == item_id)
                .unwrap_or(0);
            pane.activate_item(active_index, true, true, window, cx);
        });
        let focus_handle = pane.read(cx).focus_handle(cx);
        focus_handle.focus(window, cx);
        true
    } else {
        false
    }
}
