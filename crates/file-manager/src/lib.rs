pub(crate) mod bookmarks;
pub(crate) mod bulk_rename_editor;
pub(crate) mod file_manager;
pub(crate) mod goto_completer;
pub(crate) mod prefs;
mod view;

pub use file_manager::{
    CopyMarked, CreateDirectory, CreateFile, DeleteEntry, FileManager, GotoPath, HistoryBack,
    HistoryForward, MoveMarked, Open, RenameEntry, Reveal, ToggleMark, YankPath,
};

use gpui::App;

pub fn init(cx: &mut App) {
    file_manager::init(cx);
}
