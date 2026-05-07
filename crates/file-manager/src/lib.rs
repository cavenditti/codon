mod file_manager;

pub use file_manager::{FileManager, Open};

use gpui::App;

pub fn init(cx: &mut App) {
    file_manager::init(cx);
}
