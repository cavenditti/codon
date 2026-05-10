pub mod actions;

pub use actions::*;

use gpui::App;

pub fn init(cx: &mut App) {
    actions::register(cx);
}
