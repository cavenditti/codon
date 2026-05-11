//! Codon command palette.
//!
//! Today this crate is a thin wrapper around Zed's `command_palette`. It owns
//! the codon-side action (`codon_command_palette::Toggle`) so the codon keymap
//! has a stable name to bind `:` to, and the keymap doesn't need to know how
//! the palette is implemented underneath.
//!
//! Later tasks under `REQ:codon/command-palette` swap the body of this
//! handler for codon's own modal with an always-visible description pane and
//! typed-argument completers. The action surface, and therefore the keymap,
//! stays stable across that swap.
//!
//! See:
//! - `.specs/codon/command-palette.spec.md` — the REQ.
//! - `.specs/phase-5/command-palette-colon-trigger.spec.md` — this prototype.

use gpui::{Context, Window, actions};
use workspace::Workspace;

actions!(
    codon_command_palette,
    [
        /// Open the codon command palette. Bound to `:` in codon Normal mode
        /// and to `cmd-shift-p` globally.
        Toggle,
    ]
);

/// Register the codon command-palette action handler on a workspace.
///
/// Mirrors the pattern used by `codon_session::actions::register_for_workspace`
/// — invoked from `apps/codon::zed::initialize_workspace` once per workspace.
pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_toggle);
}

fn handle_toggle(
    _workspace: &mut Workspace,
    _: &Toggle,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Layer A scaffold: defer to Zed's command palette. The codon-owned modal
    // (description pane + completer sub-picker) will replace this body in the
    // remaining phase-5 tasks under `REQ:codon/command-palette`.
    window.dispatch_action(Box::new(zed_actions::command_palette::Toggle), cx);
}
