//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

pub mod mode_indicator;
pub mod selection;

pub use codon_pane_bridge::{
    AroundContainer, CodonModeTracker, GrammarKind, GrammarSelection, InnerContainer,
    ObjectGrammar, ObjectNext, ObjectPrev, PaneMode, PaneModeBridge, SelectAll, SwitchToCommand,
    SwitchToInsert, SwitchToNormal, install_pane_mode_dispatcher, register_pane_mode_bridge,
};
pub use command_palette_hooks::{ActionAcceptsRegistry, ObjectKind};
pub use mode_indicator::CodonModeIndicator;
pub use selection::{Selection, SelectionSource};

use gpui::App;

pub fn init(cx: &mut App) {
    cx.set_global(CodonModeTracker::default());
    cx.set_global(ActionAcceptsRegistry::default());
}
