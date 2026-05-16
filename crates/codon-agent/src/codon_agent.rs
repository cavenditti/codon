//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

pub mod actions;

pub use actions::*;

use gpui::App;

pub fn init(cx: &mut App) {
    actions::register(cx);
}
