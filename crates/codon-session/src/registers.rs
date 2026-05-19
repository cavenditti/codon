//! Error pattern: write/read/clear are infallible; invalid register names are returned via the boundary-error `RegisterNameError` and the dispatcher surfaces them with a status-bar message rather than panicking.
//!
//! Typed-selection registers — Helix's `"a y`-style register prefix
//! generalised across every pane's [`codon_mode::Selection`] vocabulary.
//!
//! ## What ships in this task
//!
//! - [`RegisterStore`] as a `gpui::Global` keyed by the active session.
//! - [`RegisterName`] — the validated single-char name vocabulary.
//! - [`SelectRegister`] — the `"<char>` action the dispatcher routes
//!   through to arm the next register-aware verb.
//! - [`PendingRegister`] — short-lived "the next selection-producing
//!   /-consuming verb should use this register" arming state, kept on
//!   the store so the keymap dispatcher and pane crates share one
//!   source of truth.
//!
//! Follow-up tasks layer in:
//! - Per-session contents persistence + the `[registers]` TOML section
//!   for named-persistent registers
//!   (`phase-19/selection-registers-persistent`).
//! - Helix text-register interop (`phase-19/selection-registers-helix-interop`).
//! - `RegisterOverview` modal (`phase-19/selection-registers-overview`).
//!
//! See `REQ:codon/selection-registers` for the full design.

use std::collections::HashMap;
use std::sync::Arc;

use codon_mode::Selection;
use gpui::{Action, App, Global};
use parking_lot::RwLock;
use schemars::JsonSchema;
use serde::Deserialize;
use thiserror::Error;

use crate::session::SessionId;

/// Boundary error for register-name validation. The dispatcher catches
/// this and emits a `"unknown register '<x>'"` status message rather
/// than crashing the focused pane.
#[derive(Debug, Error)]
pub enum RegisterNameError {
    /// The register name was not in the allowed alphabet
    /// (`[a-zA-Z0-9"_+*-]`).
    #[error("invalid register name: '{0}'")]
    Invalid(char),
}

/// Validated single-char register name. Keeps the input alphabet a
/// hard precondition of the API so `RegisterStore` callers don't have
/// to re-check on every read / write.
///
/// Constructed via [`Self::try_new`]; the dispatcher's `"<char>`
/// parsing maps to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegisterName(char);

impl RegisterName {
    /// Allowed characters: lowercase + uppercase ASCII, digits, and
    /// the small set of named slots Helix carries over (`"_+*-`). The
    /// double-quote name is the "default" register Helix uses for the
    /// most-recent yank.
    pub fn try_new(c: char) -> Result<Self, RegisterNameError> {
        if c.is_ascii_alphanumeric() || matches!(c, '"' | '_' | '+' | '*' | '-') {
            Ok(Self(c))
        } else {
            Err(RegisterNameError::Invalid(c))
        }
    }

    pub fn as_char(self) -> char {
        self.0
    }
}

impl std::fmt::Display for RegisterName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

/// The arming-state the dispatcher writes when the user types `"a`
/// and a verb hasn't yet consumed it. Cleared either when:
///
/// - the next selection-producing verb writes to the named register;
/// - the next selection-consuming verb reads from the named register;
/// - a non-register-aware verb fires (the arming is single-shot, in
///   line with Helix's `"<x>` behaviour).
///
/// Pane crates check [`RegisterStore::take_pending`] when they
/// implement a register-aware verb; the keymap dispatcher writes via
/// [`RegisterStore::arm_pending`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingRegister {
    pub name: RegisterName,
}

/// `gpui::Global` holding the cross-pane register map. Behind an
/// `Arc<RwLock<...>>` so callers can hold a cheap handle past the
/// `App` borrow — same pattern as `SessionRegistry`.
#[derive(Default, Clone)]
pub struct RegisterStore {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Default)]
struct Inner {
    active_session: Option<SessionId>,
    by_session: HashMap<SessionId, HashMap<RegisterName, Selection>>,
    pending: Option<PendingRegister>,
}

impl Global for RegisterStore {}

impl RegisterStore {
    /// Cheap (Arc-cloned) handle to the global store. The
    /// `SessionRegistry`-style ergonomic so callers can detach from
    /// the `App` borrow when they need to.
    pub fn global(cx: &App) -> RegisterStore {
        cx.global::<RegisterStore>().clone()
    }

    /// Tell the store which session is "active" — what reads / writes
    /// without an explicit session id resolve to. Called from
    /// `SessionRegistry`'s active-session swap path.
    pub fn swap_session(&self, new_active: SessionId) {
        let mut guard = self.inner.write();
        guard.active_session = Some(new_active);
        // Make sure the active session has a map slot — read can stay
        // a `HashMap::get` (which returns `None` cleanly) but it makes
        // tests / debugging clearer.
        guard.by_session.entry(new_active).or_default();
    }

    /// Drop the session's map. Called when a session is closed via
    /// `SessionRegistry::remove`; the per-session register contents
    /// don't survive (the named-persistent variant is the follow-up
    /// task, see `REQ:codon/selection-registers#c-named-persistent`).
    pub fn drop_session(&self, id: SessionId) {
        let mut guard = self.inner.write();
        guard.by_session.remove(&id);
        if guard.active_session == Some(id) {
            guard.active_session = None;
        }
    }

    /// Write a selection to the named register in the active session.
    /// No-op if no session is active — the store needs at least one
    /// `swap_session` before it can hold writes. Returns `true` on
    /// success so callers can surface "stored register 'a'" status
    /// messages.
    pub fn write(&self, name: RegisterName, value: Selection) -> bool {
        let mut guard = self.inner.write();
        let Some(active) = guard.active_session else {
            return false;
        };
        guard
            .by_session
            .entry(active)
            .or_default()
            .insert(name, value);
        // Writing a register also clears any pending arming — the
        // verb that produced this write has consumed the `"<char>`.
        guard.pending = None;
        true
    }

    /// Read the named register from the active session. Returns
    /// `None` if no session is active, the session has no map yet,
    /// or the slot is empty. Cloning happens on read so the caller
    /// can drop the lock immediately.
    pub fn read(&self, name: RegisterName) -> Option<Selection> {
        let guard = self.inner.read();
        let active = guard.active_session?;
        guard.by_session.get(&active)?.get(&name).cloned()
    }

    /// Clear a single named register in the active session.
    pub fn clear(&self, name: RegisterName) {
        let mut guard = self.inner.write();
        let Some(active) = guard.active_session else {
            return;
        };
        if let Some(map) = guard.by_session.get_mut(&active) {
            map.remove(&name);
        }
    }

    /// Arm the next register-aware verb with the given name. Called
    /// from the keymap dispatcher on a `"<char>` action; consumed by
    /// the next call to [`Self::take_pending`].
    pub fn arm_pending(&self, name: RegisterName) {
        self.inner.write().pending = Some(PendingRegister { name });
    }

    /// Take (and clear) the pending arming, if any. Single-shot: the
    /// verb that consumes the arming clears it so the next verb
    /// without a prefix doesn't accidentally pick it up.
    pub fn take_pending(&self) -> Option<PendingRegister> {
        self.inner.write().pending.take()
    }

    /// Peek at the pending arming without consuming it. Useful for the
    /// status-bar register-prefix indicator (which the follow-up task
    /// adds) so it can show "waiting for verb after \"<x>" without
    /// clearing the state.
    pub fn peek_pending(&self) -> Option<PendingRegister> {
        self.inner.read().pending
    }
}

// ---------------------------------------------------------------------------
// Dispatcher action
// ---------------------------------------------------------------------------

/// `codon_registers::SelectRegister("a")` — the `"<char>` Normal-mode
/// prefix. Payload carries the single character (string-typed so the
/// TOML keymap and JSON payload form line up — `Action` derive needs
/// `Deserialize`, and `char` doesn't round-trip cleanly from a TOML
/// string of length 1 the way `String` does).
///
/// The dispatcher handler arms the active register on the
/// [`RegisterStore`] and stops there; the actual write / read happens
/// when the next verb fires.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = codon_registers)]
#[serde(deny_unknown_fields)]
pub struct SelectRegister(pub String);

impl SelectRegister {
    /// Pull the validated `RegisterName` out of the payload. Returns
    /// `Err(RegisterNameError::Invalid)` for empty / multi-char
    /// payloads as well as the allowed-alphabet check, so the
    /// dispatcher has one branch to handle.
    pub fn name(&self) -> Result<RegisterName, RegisterNameError> {
        let mut chars = self.0.chars();
        let Some(c) = chars.next() else {
            return Err(RegisterNameError::Invalid('\0'));
        };
        if chars.next().is_some() {
            return Err(RegisterNameError::Invalid(c));
        }
        RegisterName::try_new(c)
    }
}

/// Install the global store. Idempotent: re-init (test plumbing) keeps
/// the existing instance so live registers don't get nuked.
pub fn init(cx: &mut App) {
    if !cx.has_global::<RegisterStore>() {
        cx.set_global(RegisterStore::default());
    }
}

/// Workspace-side `on_action` registration for [`SelectRegister`] —
/// arms the store's pending slot so the *next* register-aware verb
/// reads / writes the named register. Called from
/// `actions::register_for_workspace`.
pub fn register_for_workspace(workspace: &mut workspace::Workspace) {
    workspace.register_action(handle_select_register);
}

fn handle_select_register(
    _workspace: &mut workspace::Workspace,
    action: &SelectRegister,
    _window: &mut gpui::Window,
    cx: &mut gpui::Context<workspace::Workspace>,
) {
    match action.name() {
        Ok(name) => {
            let store = RegisterStore::global(cx);
            store.arm_pending(name);
            log::debug!("codon-registers: armed register '{}'", name);
        }
        Err(err) => {
            log::warn!("codon-registers: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codon_mode::Selection;
    use std::path::PathBuf;

    fn sid() -> SessionId {
        SessionId::new()
    }

    #[test]
    fn register_name_allowed_alphabet_round_trips() {
        for c in ['a', 'Z', '0', '"', '_', '+', '*', '-'] {
            let name = RegisterName::try_new(c).expect("allowed char");
            assert_eq!(name.as_char(), c);
        }
    }

    #[test]
    fn register_name_rejects_disallowed() {
        for c in [' ', '#', '\n', '🦀'] {
            assert!(matches!(
                RegisterName::try_new(c),
                Err(RegisterNameError::Invalid(_))
            ));
        }
    }

    #[test]
    fn write_then_read_round_trips_in_active_session() {
        let store = RegisterStore::default();
        let s = sid();
        store.swap_session(s);
        let name = RegisterName::try_new('f').unwrap();
        let value = Selection::Files(vec![PathBuf::from("/a")]);
        assert!(store.write(name, value.clone()));
        let got = store.read(name).expect("readable");
        match got {
            Selection::Files(paths) => assert_eq!(paths, vec![PathBuf::from("/a")]),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn write_without_active_session_is_a_noop() {
        let store = RegisterStore::default();
        let name = RegisterName::try_new('f').unwrap();
        assert!(!store.write(name, Selection::Files(vec![PathBuf::from("/x")])));
        assert!(store.read(name).is_none());
    }

    #[test]
    fn session_swap_preserves_inactive_sessions_data() {
        let store = RegisterStore::default();
        let a = sid();
        let b = sid();

        store.swap_session(a);
        let name = RegisterName::try_new('f').unwrap();
        store.write(name, Selection::Files(vec![PathBuf::from("/a")]));

        // Switch to b — A's write should NOT be visible from B.
        store.swap_session(b);
        assert!(store.read(name).is_none());
        store.write(name, Selection::Files(vec![PathBuf::from("/b")]));
        let got_b = store.read(name).unwrap();
        let Selection::Files(paths_b) = got_b else {
            panic!("wrong");
        };
        assert_eq!(paths_b, vec![PathBuf::from("/b")]);

        // Switching back to A should restore A's value.
        store.swap_session(a);
        let got_a = store.read(name).unwrap();
        let Selection::Files(paths_a) = got_a else {
            panic!("wrong");
        };
        assert_eq!(paths_a, vec![PathBuf::from("/a")]);
    }

    #[test]
    fn clear_removes_entry() {
        let store = RegisterStore::default();
        let s = sid();
        store.swap_session(s);
        let name = RegisterName::try_new('f').unwrap();
        store.write(name, Selection::Files(vec![PathBuf::from("/x")]));
        store.clear(name);
        assert!(store.read(name).is_none());
    }

    #[test]
    fn drop_session_removes_its_map_and_unsets_active() {
        let store = RegisterStore::default();
        let s = sid();
        store.swap_session(s);
        let name = RegisterName::try_new('f').unwrap();
        store.write(name, Selection::Files(vec![PathBuf::from("/x")]));
        store.drop_session(s);
        assert!(store.read(name).is_none());
        // Re-arming the same session id starts empty.
        store.swap_session(s);
        assert!(store.read(name).is_none());
    }

    #[test]
    fn pending_arming_is_single_shot() {
        let store = RegisterStore::default();
        let name = RegisterName::try_new('a').unwrap();
        store.arm_pending(name);
        let taken = store.take_pending().expect("armed");
        assert_eq!(taken.name, name);
        assert!(store.take_pending().is_none(), "single-shot");
    }

    #[test]
    fn writing_a_register_clears_pending() {
        let store = RegisterStore::default();
        let s = sid();
        store.swap_session(s);
        let name = RegisterName::try_new('a').unwrap();
        store.arm_pending(name);
        store.write(name, Selection::Files(vec![]));
        assert!(store.take_pending().is_none());
    }

    #[test]
    fn select_register_action_payload_validates() {
        let action = SelectRegister("a".into());
        assert_eq!(action.name().unwrap().as_char(), 'a');
        let bad = SelectRegister("".into());
        assert!(bad.name().is_err());
        let too_long = SelectRegister("ab".into());
        assert!(too_long.name().is_err());
        let symbol = SelectRegister("#".into());
        assert!(symbol.name().is_err());
    }
}
