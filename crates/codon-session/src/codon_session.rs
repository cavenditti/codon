//! Error pattern: `SessionRegistryError` at the `registry` module's public API (boundary errors callers can match on); `anyhow::Result` everywhere else.

pub mod actions;
pub mod break_pane;
pub mod contextual_split;
pub mod diff_open;
pub mod goto_or_open;
pub mod git_branch_indicator;
pub mod overview;
pub mod pane_context_label;
pub mod picker;
pub mod registry;
pub mod resize_sticky;
pub mod runtime;
pub mod session;
pub mod status_item;
pub mod swap;
pub mod window_indicator;
pub mod window_picker;
pub mod window_rename;

pub use actions::*;
pub use git_branch_indicator::GitBranchIndicator;
pub use pane_context_label::PaneContextLabel;
pub use picker::SessionSwitchModal;
pub use registry::{SessionRegistry, SessionRegistryError};
pub use session::{Session, SessionId, Window, WindowId};
pub use status_item::SessionStatusItem;
pub use window_indicator::WindowsStatusItem;
pub use workspace::codon_bridge::LayoutSnapshot;

use gpui::App;

pub fn init(cx: &mut App) {
    // Codon owns its own close-cascade (see `SafeCloseActiveItem`), so the
    // Zed-side auto-`CloseWindow` branch on an empty pane is muted: an
    // accidental cmd-w never collapses the OS window.
    workspace::pane::set_close_window_on_last_tab(false);
    actions::register(cx);
    registry::init(cx);
    runtime::init(cx);
}
