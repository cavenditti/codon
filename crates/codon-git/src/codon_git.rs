//! codon-git — lightweight git panes for codon.
//!
//! Spec: [REQ:codon/git-pane](../../../.specs/codon/git-pane.spec.md).
//!
//! This crate is a *new* surface, not a fork of `git_ui::GitPanel`. It
//! reuses domain types from `git::` and `project::git_store::` but the
//! rendering / keymap / pane lifecycle is fresh, so we don't carry the
//! 6000-line sidebar panel into codon's pane tree.

mod git_status_pane;

pub use git_status_pane::{
    GitStatusPane, GoToBottom, GoToTop, NavigateDown, NavigateUp, Open, OpenStatusPane, Stage,
    Unstage,
};

use gpui::App;

pub fn init(cx: &mut App) {
    git_status_pane::init(cx);
}
