//! Transient peek dock surface.
//!
//! The peek surface re-uses Zed's existing `Dock` widget rather than
//! standing up a separate floating overlay. Each side dock (left, right,
//! bottom) on the codon workspace is normally empty (see the
//! `dock-deprecation` task in `apps/codon/src/zed.rs`); a peek invocation
//! drops a panel into the matching dock, opens it, and routes focus.
//!
//! v1 deviation from the spec: focus-loss auto-dismiss is *not*
//! implemented — re-invoking the same `Peek<Name>` action or
//! `codon_panes::PeekDismiss` (bound to `esc` while the peek dock has
//! focus) is the only way to close it. The dispatch-context predicate
//! `peek_dock` in the keymap drives the `esc` binding. This keeps the
//! peek surface working without a new vendored-Zed observer hook;
//! auto-dismiss can be layered on later by subscribing to workspace
//! focus changes.

use gpui::{App, Context, Entity, Window};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel},
};

use crate::actions::PeekDismiss;

/// Side of the workspace a peek anchors to. Maps onto Zed's `DockPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeekSide {
    Left,
    Right,
    Bottom,
}

impl PeekSide {
    fn into_dock_position(self) -> DockPosition {
        match self {
            PeekSide::Left => DockPosition::Left,
            PeekSide::Right => DockPosition::Right,
            PeekSide::Bottom => DockPosition::Bottom,
        }
    }
}

/// Open `panel` as a transient peek on the requested side. If the panel
/// is already mounted in a workspace dock, the visibility of the matching
/// dock is toggled (mirrors tmux popup behaviour). When `panel` is new
/// to this workspace, it is registered via `Workspace::add_panel` before
/// the dock is opened.
pub fn peek_panel<P: Panel>(
    workspace: &mut Workspace,
    panel: Entity<P>,
    side: PeekSide,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let existing = workspace.panel::<P>(cx);
    let needs_registration = existing
        .as_ref()
        .map(|existing| existing.entity_id() != panel.entity_id())
        .unwrap_or(true);

    if needs_registration {
        // Force the panel into the requested dock position before adding —
        // `add_panel` reads the panel's `position` to choose its host dock,
        // so the desired peek side becomes the dock position for now.
        panel.update(cx, |panel, cx| {
            if panel.position_is_valid(side.into_dock_position()) {
                panel.set_position(side.into_dock_position(), window, cx);
            }
        });
        workspace.add_panel(panel, window, cx);
    }

    // `toggle_panel_focus` opens the matching dock when the panel is
    // hidden, focuses it, and closes it again when re-invoked. That's
    // exactly the peek-toggle contract from the spec.
    workspace.toggle_panel_focus::<P>(window, cx);
}

/// Close the active peek, if any. Walks every dock and closes the one
/// that currently contains the focused panel. Falls through silently if
/// no peek is open — `esc` then propagates normally.
pub fn dismiss_active_peek(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Find which dock currently holds the focus, if any.
    let candidate = [
        (workspace.left_dock().clone(), "left"),
        (workspace.right_dock().clone(), "right"),
        (workspace.bottom_dock().clone(), "bottom"),
    ];
    for (dock, _label) in candidate {
        let is_open = dock.read(cx).is_open();
        let has_focus = dock.read(cx).focus_handle(cx).contains_focused(window, cx);
        if is_open && has_focus {
            dock.update(cx, |dock, cx| {
                dock.set_open(false, window, cx);
            });
            return;
        }
    }
}

/// Workspace-level handler for `PeekDismiss`. Hooked in `register_for_workspace`.
pub(crate) fn handle_peek_dismiss(
    workspace: &mut Workspace,
    _: &PeekDismiss,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    dismiss_active_peek(workspace, window, cx);
}

/// `Focusable` lives behind a different import root in newer GPUI — keep
/// the import local to this module.
use gpui::Focusable as _;

#[allow(dead_code)]
fn _silence_unused(_: &App) {}
