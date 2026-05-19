//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

pub mod changed_files;
pub mod dir_picker;
pub mod jumplist;
pub mod last_picker;
pub mod open_rewire;
pub mod scaffold;

pub use changed_files::ChangedFilesPicker;
pub use dir_picker::{
    DirPickerDelegate, DirPickerModal, DirSelected, FilesSelected, ToggleMark,
};
pub use jumplist::JumplistPicker;
pub use last_picker::{LastPicker, record_dismissed, take_query_for};
pub use scaffold::{ModalModeTag, ModalScaffold};

use gpui::App;

pub fn init(cx: &mut App) {
    dir_picker::register_default_keybindings(cx);
    open_rewire::init(cx);
    changed_files::init(cx);
    jumplist::init(cx);
    last_picker::init(cx);
}
