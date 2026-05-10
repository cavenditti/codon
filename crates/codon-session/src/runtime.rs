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

use gpui::{App, Entity, Global};
use parking_lot::Mutex;
use workspace::{Member, Pane};

use crate::session::{SessionId, WindowId};

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
}

pub fn init(cx: &mut App) {
    cx.set_global(WindowRuntimeCache::default());
}
