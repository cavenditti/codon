//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

mod cheatsheet_modal;
mod keymap;
mod passthrough;

pub use cheatsheet_modal::{KeybindingsCheatsheetModal, ShowKeymap};
pub use keymap::{CuratedBinding, codon_default_bindings, codon_user_bindings, load_codon_keymap};
pub use passthrough::SendPrefixToFocus;

use gpui::App;
use workspace::Workspace;

/// Install codon-keymap's App-level wiring. Must be called once at
/// app startup, after `codon_mode::install_pane_mode_dispatcher`.
///
/// This intentionally does **not** call [`load_codon_keymap`] — that
/// is driven by `codon-config`'s watcher (initial load + reload on
/// change), so wiring it here would double-load on startup.
pub fn init(cx: &mut App) {
    codon_mode::register_pane_mode_bridge::<KeybindingsCheatsheetModal>(cx);
}

/// Wire codon-keymap's workspace-scoped action handlers — the
/// cheatsheet modal (`ShowKeymap`) and the double-prefix passthrough
/// (`SendPrefixToFocus`). Called from
/// `apps/codon/src/zed.rs`'s `observe_new::<Workspace>` chain.
pub fn register_for_workspace(workspace: &mut Workspace) {
    cheatsheet_modal::register_for_workspace(workspace);
    passthrough::register_for_workspace(workspace);
}
