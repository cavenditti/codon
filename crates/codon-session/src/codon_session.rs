pub mod actions;
pub mod picker;
pub mod registry;
pub mod runtime;
pub mod session;
pub mod status_item;
pub mod swap;
pub mod window_indicator;

pub use actions::*;
pub use picker::SessionSwitchModal;
pub use registry::{SessionRegistry, SessionRegistryError};
pub use session::{Session, SessionId, Window, WindowId};
pub use status_item::SessionStatusItem;
pub use window_indicator::WindowsStatusItem;
pub use workspace::codon_bridge::LayoutSnapshot;

use gpui::App;

pub fn init(cx: &mut App) {
    actions::register(cx);
    registry::init(cx);
    runtime::init(cx);
}
