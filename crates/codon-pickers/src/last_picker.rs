//! Last-picker singleton — `codon_pickers::LastPicker`.
//!
//! Helix's `space '` reopens the most recently dismissed picker with its
//! prior query intact. Codon-owned pickers stash their dismissed state
//! into the singleton via [`record_dismissed`] from their
//! `PickerDelegate::dismissed` impl; the [`LastPicker`] action reads the
//! singleton and dispatches the recorded action through the focused
//! window, which lets the picker's toggle handler seed its own query
//! via [`take_query_for`].
//!
//! Scope (matches the spec):
//!
//! - **Codon-owned pickers only.** Vendored Zed pickers do not record
//!   into this singleton; wrapping every upstream `PickerDelegate::dismissed`
//!   would require touching ~10 separate impls and is out of scope.
//! - **Per-workspace ergonomic, but in practice global.** GPUI's
//!   `Global` lives on the `App`; codon runs a single workspace per
//!   `App` so there is no observable difference between "global" and
//!   "per-workspace" today.
//! - **No persistence across restarts.** The stash lives in `App`
//!   globals; it is gone when the process exits.
//!
//! See `TASK:phase-16/pickers-last-picker` and
//! `REQ:codon/helix-pickers#c-last-picker`.

use std::sync::Arc;

use gpui::{App, Context, Global, SharedString, Window, actions};
use parking_lot::Mutex;
use workspace::Workspace;

actions!(
    codon_pickers,
    [
        /// Reopen the most recently dismissed codon picker with the
        /// previous query restored.
        LastPicker,
    ]
);

/// Stash of the last codon picker that closed.
///
/// `action_name` is the qualified action id (`"codon_pickers::JumplistPicker"`),
/// the same string GPUI's action registry uses. Pickers fill this in
/// themselves; the `LastPicker` reopen handler (follow-up task) will
/// read it back and dispatch the matching `Action` through the focused
/// element.
#[derive(Clone, Debug, Default)]
pub(crate) struct LastPickerState {
    pub(crate) action_name: Option<SharedString>,
    pub(crate) query: SharedString,
}

#[derive(Default)]
pub(crate) struct LastPickerStore {
    pub(crate) state: Arc<Mutex<LastPickerState>>,
}

impl Global for LastPickerStore {}

pub(crate) fn store(cx: &mut App) -> Arc<Mutex<LastPickerState>> {
    if !cx.has_global::<LastPickerStore>() {
        cx.set_global(LastPickerStore::default());
    }
    cx.global::<LastPickerStore>().state.clone()
}

/// Codon-owned pickers call this from their `PickerDelegate::dismissed`
/// impl with the qualified action name (`"codon_pickers::<Action>"`) and
/// the current query string. The most recent call wins; pickers do not
/// need to coordinate.
pub fn record_dismissed<T>(cx: &mut Context<T>, action_name: &str, query: SharedString)
where
    T: 'static,
{
    let state = store(cx);
    let mut guard = state.lock();
    guard.action_name = Some(SharedString::from(action_name.to_string()));
    guard.query = query;
}

/// Read-and-clear the recorded query for `action_name`. The query lives
/// in the singleton until consumed; pickers call this from their toggle
/// handler to seed the freshly opened modal. Returns `None` if no picker
/// of that name has been recorded or if the recorded query is empty.
pub fn take_query_for(cx: &mut App, action_name: &str) -> Option<SharedString> {
    let state = store(cx);
    let mut guard = state.lock();
    let recorded = guard.action_name.as_ref()?;
    if recorded.as_ref() != action_name {
        return None;
    }
    let query = std::mem::take(&mut guard.query);
    if query.is_empty() { None } else { Some(query) }
}

/// Read the action name + query verbatim without clearing. The
/// `LastPicker` reopen handler uses this to look up which picker
/// action to dispatch; the picker's own toggle handler then calls
/// [`take_query_for`] to actually consume the query.
fn recorded_action_name(cx: &mut App) -> Option<SharedString> {
    let state = store(cx);
    let guard = state.lock();
    guard.action_name.clone()
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(handle_reopen);
    })
    .detach();
}

fn handle_reopen(
    _workspace: &mut Workspace,
    _: &LastPicker,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(action_name) = recorded_action_name(cx) else {
        return;
    };
    // GPUI looks up actions by their qualified name (e.g.
    // `codon_pickers::JumplistPicker`). `build_action` returns the
    // typed `Box<dyn Action>` ready for dispatch. Unknown names — i.e.
    // a picker that recorded a name but never registered an action —
    // fall through to a no-op rather than panicking.
    let Ok(action) = cx.build_action(&action_name, None) else {
        return;
    };
    window.dispatch_action(action, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, TestAppContext};

    struct Host;

    fn install(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(LastPickerStore::default());
        });
    }

    #[gpui::test]
    async fn record_then_take_returns_query(cx: &mut TestAppContext) {
        install(cx);
        let host = cx.new(|_| Host);
        host.update(cx, |_, cx| {
            record_dismissed(cx, "codon_pickers::ChangedFilesPicker", "needle".into());
        });
        cx.update(|cx| {
            let taken = take_query_for(cx, "codon_pickers::ChangedFilesPicker");
            assert_eq!(taken.as_deref(), Some("needle"));
        });
    }

    #[gpui::test]
    async fn take_returns_none_for_mismatched_action(cx: &mut TestAppContext) {
        install(cx);
        let host = cx.new(|_| Host);
        host.update(cx, |_, cx| {
            record_dismissed(cx, "codon_pickers::ChangedFilesPicker", "abc".into());
        });
        cx.update(|cx| {
            assert!(take_query_for(cx, "codon_pickers::JumplistPicker").is_none());
            assert_eq!(
                take_query_for(cx, "codon_pickers::ChangedFilesPicker").as_deref(),
                Some("abc")
            );
        });
    }

    #[gpui::test]
    async fn take_returns_none_for_empty_query(cx: &mut TestAppContext) {
        install(cx);
        let host = cx.new(|_| Host);
        host.update(cx, |_, cx| {
            record_dismissed(cx, "codon_pickers::ChangedFilesPicker", "".into());
        });
        cx.update(|cx| {
            assert!(take_query_for(cx, "codon_pickers::ChangedFilesPicker").is_none());
        });
    }

    #[gpui::test]
    async fn last_record_wins(cx: &mut TestAppContext) {
        install(cx);
        let host = cx.new(|_| Host);
        host.update(cx, |_, cx| {
            record_dismissed(cx, "codon_pickers::ChangedFilesPicker", "first".into());
            record_dismissed(cx, "codon_pickers::JumplistPicker", "second".into());
        });
        cx.update(|cx| {
            assert_eq!(
                take_query_for(cx, "codon_pickers::JumplistPicker").as_deref(),
                Some("second")
            );
        });
    }
}
