pub mod mode_indicator;
pub mod pane_mode;
pub mod selection;

pub use command_palette_hooks::{ActionAcceptsRegistry, ObjectKind};
pub use mode_indicator::CodonModeIndicator;
pub use pane_mode::{CodonModeTracker, PaneMode, SwitchToCommand, SwitchToInsert, SwitchToNormal};
pub use selection::{Selection, SelectionSource};

use gpui::App;

pub fn init(cx: &mut App) {
    cx.set_global(CodonModeTracker::default());
    cx.set_global(ActionAcceptsRegistry::default());
}
