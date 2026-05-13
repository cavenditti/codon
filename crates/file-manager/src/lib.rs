pub(crate) mod bookmarks;
pub(crate) mod file_manager;
pub(crate) mod goto_completer;
mod view;

pub use file_manager::{
    CopyMarked, CreateDirectory, CreateFile, DeleteEntry, FileManager, GotoPath, HistoryBack,
    HistoryForward, MoveMarked, Open, RenameEntry, Reveal, ToggleMark, YankPath,
};

use gpui::App;

pub fn init(cx: &mut App) {
    file_manager::init(cx);
}
