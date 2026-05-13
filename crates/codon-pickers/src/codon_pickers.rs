pub mod dir_picker;
pub mod open_rewire;

pub use dir_picker::{
    DirPickerDelegate, DirPickerModal, DirSelected, FilesSelected, ToggleMark,
};

use gpui::App;

pub fn init(cx: &mut App) {
    dir_picker::register_default_keybindings(cx);
    open_rewire::init(cx);
}
