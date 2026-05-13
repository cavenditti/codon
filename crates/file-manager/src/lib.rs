pub(crate) mod bookmarks;
pub(crate) mod file_manager;
mod view;

pub use file_manager::{
    CopyMarked, CreateDirectory, CreateFile, DeleteEntry, FileManager, HistoryBack,
    HistoryForward, MoveMarked, Open, RenameEntry, Reveal, ToggleMark, YankPath,
};

use gpui::App;

pub fn init(cx: &mut App) {
    file_manager::init(cx);
}
