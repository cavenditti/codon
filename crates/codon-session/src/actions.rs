use gpui::{App, actions};

actions!(
    codon_session,
    [
        /// Open a picker to create a new session.
        SessionNew,
        /// Open a fuzzy picker to switch sessions.
        SessionSwitch,
        /// Rename the active session.
        SessionRename,
        /// Close the active session (refuses to remove the last one).
        SessionClose,
        /// Add a new window to the active session.
        WindowNew,
        /// Move to the next window in the active session.
        WindowNext,
        /// Move to the previous window in the active session.
        WindowPrev,
        /// Close the active window in the active session.
        WindowClose,
    ]
);

pub fn register(_cx: &mut App) {
    // Action handlers are wired per-workspace from the entry crate so they
    // have access to a Workspace handle. Defining the action types here keeps
    // them globally dispatchable and avoids name conflicts with vendored
    // actions.
}
