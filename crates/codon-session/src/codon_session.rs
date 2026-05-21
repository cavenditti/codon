//! Error pattern: `SessionRegistryError` at the `registry` module's public API (boundary errors callers can match on); `anyhow::Result` everywhere else.

pub mod actions;
pub mod break_pane;
pub mod contextual_split;
pub mod diff_open;
pub mod goto_or_open;
pub mod overview;
pub mod pane_context_label;
pub mod picker;
pub mod registers;
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
pub use pane_context_label::PaneContextLabel;
pub use picker::SessionSwitchModal;
pub use registers::{
    PendingRegister, RegisterName, RegisterNameError, RegisterStore, SelectRegister,
};
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
    registers::init(cx);
    runtime::init(cx);

    // Wire the vendored Zed `restore_center_root` timing callback into
    // the switch-perf trace harness. The callback captures the wall-clock
    // duration of the restore burst plus the count of previously-unseen
    // panes attached to the workspace; the value lands in a thread-local
    // slot that the codon-session action handler drains when it builds
    // the corresponding `SwitchTiming` event. The harness pattern matches
    // the existing codon pane-kind registry: vendored Zed does not import
    // codon types, codon installs the function pointer here.
    workspace::codon_bridge::set_restore_timing_callback(
        crate::actions::record_restore_timing_from_workspace,
    );
}
