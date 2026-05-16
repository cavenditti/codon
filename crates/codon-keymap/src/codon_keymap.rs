mod cheatsheet_modal;
mod keymap;

pub use cheatsheet_modal::{KeybindingsCheatsheetModal, ShowKeymap, register_for_workspace};
pub use keymap::{CuratedBinding, codon_default_bindings, codon_user_bindings, load_codon_keymap};

use gpui::App;

/// Install codon-keymap's App-level wiring. Must be called once at
/// app startup, after `codon_mode::install_pane_mode_dispatcher`.
///
/// This intentionally does **not** call [`load_codon_keymap`] — that
/// is driven by `codon-config`'s watcher (initial load + reload on
/// change), so wiring it here would double-load on startup.
pub fn init(cx: &mut App) {
    codon_mode::register_pane_mode_bridge::<KeybindingsCheatsheetModal>(cx);
}
