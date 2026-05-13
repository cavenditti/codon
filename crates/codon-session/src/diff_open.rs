//! Codon-side entry point for opening a diff view.
//!
//! Today this is a thin shim: it dispatches Zed's upstream `git::Diff`
//! action, which opens the `ProjectDiff` multi-buffer view showing every
//! working-tree change vs HEAD. That covers the most common "show me what
//! changed" workflow — the same view a user gets from the git panel.
//!
//! Scope deferred until `TASK:phase-4/git-diff-pane` lands:
//!   * Arbitrary file-vs-file true-diff (e.g. open two file-manager
//!     selections side-by-side as a diff multi-buffer).
//!   * Buffer-vs-disk diffs scoped to one buffer.
//!   * Branch-vs-branch diffs with picker.
//! Once the dedicated codon diff pane exists, this handler will route to
//! it with the right inputs instead of falling through to the git pane.

use gpui::{Context, Window};
use workspace::Workspace;

use crate::actions::DiffOpen;

pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_diff_open);
}

fn handle_diff_open(
    _workspace: &mut Workspace,
    _: &DiffOpen,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // `git::Diff` is declared in `git_ui::project_diff` and registered on
    // every workspace by `git_ui::init`. Dispatching it from here gives
    // codon the indirection it needs to swap in a richer diff pane later
    // without touching the keymap.
    window.dispatch_action(Box::new(git_ui::project_diff::Diff), cx);
}
