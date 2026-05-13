mod cheatsheet_modal;
mod keymap;

pub use cheatsheet_modal::{KeybindingsCheatsheetModal, ShowKeymap, register_for_workspace};
pub use keymap::{CuratedBinding, codon_default_bindings, codon_user_bindings, load_codon_keymap};

use gpui::App;

pub fn init(cx: &mut App) {
    load_codon_keymap(cx);
}
