//! Error pattern: `anyhow::Result` with `.context()` at `?` boundaries; user-driven failures surface as toasts via the workspace. No custom error types.

pub(crate) mod bookmarks;
pub(crate) mod bulk_rename_editor;
pub(crate) mod debounced_writer;
pub(crate) mod file_manager;
pub(crate) mod goto_completer;
pub(crate) mod jump_provider;
pub(crate) mod opener_picker;
pub(crate) mod openers;
pub(crate) mod persistence;
pub(crate) mod prefs;
pub(crate) mod render;
pub(crate) mod search;
pub(crate) mod shell;
pub(crate) mod task_history_modal;
pub(crate) mod tasks;
pub(crate) mod theme;
pub(crate) mod trash;
mod view;

pub use file_manager::{
    ChooseOpener, CopyMarked, CreateDirectory, CreateFile, DeleteEntry, FileManager, GotoPath,
    HistoryBack, HistoryForward, MoveMarked, Open, RenameEntry, Reveal, SortByBtime,
    SortByExtension, SortByMtime, SortByName, SortByNatural, SortByRandom, SortBySize, ToggleMark,
    ToggleSortReverse, YankPath,
};
pub use openers::{Opener, OpenerStore};
pub use render::trace::{
    CacheOutcome, SwitchKind, default_trace_path, install_global as install_render_trace,
    record_switch as record_switch_timing,
};

use std::sync::Arc;

use fs::Fs;
use gpui::App;

pub fn init(fs: Arc<dyn Fs>, cx: &mut App) {
    file_manager::init(cx);
    cx.on_app_quit(|cx| {
        cx.global::<prefs::FmPrefs>().flush();
        cx.global::<bookmarks::BookmarkStore>().flush();
        async {
            render::trace::flush_global();
        }
    })
    .detach();
    openers::init(fs.clone(), cx);
    theme::init(fs, cx);
    tasks::init(cx);
    codon_mode::register_pane_mode_bridge::<FileManager>(cx);
}
