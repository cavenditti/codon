//! Codon-namespaced action structs for the panes-from-panels surface.
//!
//! Two actions per converted panel — `Open<Name>` opens the panel as a
//! workspace pane (via `PanelItemAdapter`), `Peek<Name>` opens it in the
//! transient peek dock. Plus `PeekDismiss` to close any active peek.
//!
//! The dispatch handlers live in the per-panel modules (`agent.rs`,
//! `git.rs`, `outline.rs`, `debug.rs`) and in `peek.rs`.

use gpui::actions;

actions!(
    codon_panes,
    [
        /// Open the agent panel as a workspace pane (in the active pane
        /// split). Focuses the existing tab if one is already mounted.
        OpenAgent,
        /// Open the agent panel as a transient right-side peek.
        PeekAgent,
        /// Open the git panel as a workspace pane.
        OpenGit,
        /// Open the git panel as a transient left-side peek.
        PeekGit,
        /// Open the outline panel as a workspace pane.
        OpenOutline,
        /// Open the outline panel as a transient left-side peek.
        PeekOutline,
        /// Open the debug panel as a workspace pane.
        OpenDebug,
        /// Open the debug panel as a transient bottom peek.
        PeekDebug,
        /// Dismiss the currently visible peek, if any.
        PeekDismiss,
        /// Consume a `Selection::Files` from the register armed via
        /// the `"<char>` prefix and open each file in the workspace.
        /// No-op when no register is armed or the register doesn't
        /// hold a `Files` selection (the dispatcher logs a debug
        /// message; the user sees no visible change).
        ///
        /// `"f` then `OpenFromRegister` is the canonical flow.
        OpenFromRegister,
    ]
);
