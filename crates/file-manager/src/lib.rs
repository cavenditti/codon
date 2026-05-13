pub(crate) mod bookmarks;
pub(crate) mod bulk_rename_editor;
pub(crate) mod file_manager;
pub(crate) mod goto_completer;
pub(crate) mod openers;
pub(crate) mod prefs;
pub(crate) mod search;
pub(crate) mod shell;
pub(crate) mod trash;
mod view;

pub use file_manager::{
    ChooseOpener, CopyMarked, CreateDirectory, CreateFile, DeleteEntry, FileManager, GotoPath,
    HistoryBack, HistoryForward, MoveMarked, Open, RenameEntry, Reveal, ToggleMark, YankPath,
};
pub use openers::{Opener, OpenerStore};

use std::sync::Arc;

use fs::Fs;
use gpui::App;

pub fn init(fs: Arc<dyn Fs>, cx: &mut App) {
    file_manager::init(cx);
    openers::init(fs, cx);
}
