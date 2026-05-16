pub mod mode_indicator;
pub mod selection;

pub use codon_pane_bridge::{
    CodonModeTracker, PaneMode, PaneModeBridge, SwitchToCommand, SwitchToInsert, SwitchToNormal,
    install_pane_mode_dispatcher, register_pane_mode_bridge,
};
pub use command_palette_hooks::{ActionAcceptsRegistry, ObjectKind};
pub use mode_indicator::CodonModeIndicator;
pub use selection::{Selection, SelectionSource};

use gpui::App;

pub fn init(cx: &mut App) {
    cx.set_global(CodonModeTracker::default());
    cx.set_global(ActionAcceptsRegistry::default());
}
