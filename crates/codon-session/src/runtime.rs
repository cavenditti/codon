//! In-memory state attached to a window during a session's lifetime.
//!
//! Codon's window-switching uses this in preference to the SQLite-backed
//! snapshot path: live `Entity<Pane>` references are stashed when a window
//! becomes inactive and restored when it becomes active again, so file
//! manager / editor state survives a switch even though the items haven't
//! been written to their per-kind tables.
//!
//! The cache is *not* persisted across restarts. After a restart, codon
//! falls back to `Window::layout` (the serde snapshot) and Zed's normal
//! per-item DB rehydration.

use std::{collections::HashMap, sync::Arc};

use gpui::{App, Entity, Global, Window};
use parking_lot::Mutex;
use workspace::{Member, Pane, codon_bridge};

use crate::{
    registry::SessionRegistry,
    session::{SessionId, WindowId},
};

#[derive(Clone)]
pub struct WindowRuntime {
    pub root: Member,
    pub active_pane: Option<Entity<Pane>>,
}

#[derive(Default, Clone)]
pub struct WindowRuntimeCache {
    inner: Arc<Mutex<HashMap<(SessionId, WindowId), WindowRuntime>>>,
}

impl Global for WindowRuntimeCache {}

impl WindowRuntimeCache {
    pub fn global(cx: &App) -> WindowRuntimeCache {
        cx.global::<WindowRuntimeCache>().clone()
    }

    pub fn insert(&self, session: SessionId, window: WindowId, runtime: WindowRuntime) {
        self.inner.lock().insert((session, window), runtime);
    }

    pub fn take(&self, session: SessionId, window: WindowId) -> Option<WindowRuntime> {
        self.inner.lock().remove(&(session, window))
    }

    /// Drop every cached runtime that belongs to `session`. Used when a
    /// session is closed.
    pub fn drop_session(&self, session: SessionId) {
        self.inner.lock().retain(|(s, _), _| *s != session);
    }

    /// Materialize the cached `Member` tree for `(session, window)` into
    /// a fresh `LayoutSnapshot` and write it back to the session
    /// registry. Used when a stashed runtime entry is about to lose its
    /// "this is the freshest copy" status — LRU eviction, explicit
    /// `detach_session`, or shutdown drain.
    ///
    /// `c-skip-capture-on-cache-hit` lets the switch fast path elide
    /// `swap::capture` because the runtime cache holds the live tree.
    /// That trade off shifts the snapshot cost onto these eviction
    /// boundaries; this method is the single chokepoint that pays it.
    ///
    /// Returns `true` if an entry existed and was materialised + the
    /// registry write succeeded; `false` if no entry was cached for the
    /// pair (and therefore nothing to do).
    pub fn evict_and_persist(
        &self,
        session_id: SessionId,
        window_id: WindowId,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(runtime) = self.take(session_id, window_id) else {
            return false;
        };
        let snapshot = codon_bridge::capture_from_member(&runtime.root, window, cx);

        let registry = SessionRegistry::global(cx);
        let Some(mut session) = registry.get(session_id) else {
            log::warn!(
                "evict_and_persist: session {session_id} disappeared between cache hit and registry read"
            );
            return false;
        };
        let Some(target) = session.windows.iter_mut().find(|w| w.id == window_id) else {
            log::warn!("evict_and_persist: window {window_id:?} missing from session {session_id}");
            return false;
        };
        target.layout = Some(snapshot);
        target.layout_stale = false;
        if let Err(err) = registry.upsert(session) {
            log::warn!("evict_and_persist: failed to upsert session after materialise: {err:?}");
            return false;
        }
        true
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(WindowRuntimeCache::default());
}
