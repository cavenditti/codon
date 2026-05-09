mod file_manager;

pub use file_manager::{
    CopyMarked, CreateDirectory, CreateFile, DeleteEntry, FileManager, MoveMarked, Open,
    RenameEntry, ToggleMark, YankPath,
};

use gpui::App;

pub fn init(cx: &mut App) {
    file_manager::init(cx);
}
