mod keymap;

pub use keymap::load_codon_keymap;

use gpui::App;

pub fn init(cx: &mut App) {
    load_codon_keymap(cx);
}
