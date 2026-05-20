use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use db::kvp::GlobalKeyValueStore;
use gpui::{App, AppContext as _, Global, Task};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::session::{Session, SessionId};

const KVP_KEY: &str = "codon_sessions_v1";

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionRegistryError {
    #[error("session not found")]
    NotFound,
    #[error("name already in use")]
    DuplicateName,
    #[error("cannot remove last remaining session")]
    LastSession,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedRegistry {
    #[serde(default)]
    sessions: Vec<Session>,
    #[serde(default)]
    active: Option<SessionId>,
}

#[derive(Default, Clone)]
pub struct SessionRegistry {
    inner: Arc<RwLock<PersistedRegistry>>,
}

impl Global for SessionRegistry {}

impl SessionRegistry {
    /// Get a cheap (Arc-cloned) handle to the registry. Cloning detaches from
    /// the `App` borrow so callers can hold the handle across `cx.notify()`,
    /// `swap::capture(..)`, etc.
    pub fn global(cx: &App) -> SessionRegistry {
        cx.global::<SessionRegistry>().clone()
    }

    pub fn sessions(&self) -> Vec<Session> {
        self.inner.read().sessions.clone()
    }

    pub fn active_id(&self) -> Option<SessionId> {
        self.inner.read().active
    }

    pub fn active(&self) -> Option<Session> {
        let guard = self.inner.read();
        guard.active.and_then(|id| {
            guard
                .sessions
                .iter()
                .find(|session| session.id == id)
                .cloned()
        })
    }

    pub fn get(&self, id: SessionId) -> Option<Session> {
        self.inner
            .read()
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
    }

    pub fn upsert(&self, session: Session) -> Result<(), SessionRegistryError> {
        let mut guard = self.inner.write();
        if let Some(existing) = guard
            .sessions
            .iter()
            .find(|s| s.name == session.name && s.id != session.id)
        {
            log::warn!(
                "session name collision: '{}' already used by {}",
                existing.name,
                existing.id
            );
            return Err(SessionRegistryError::DuplicateName);
        }
        if let Some(slot) = guard.sessions.iter_mut().find(|s| s.id == session.id) {
            *slot = session;
        } else {
            guard.sessions.push(session);
        }
        Ok(())
    }

    pub fn remove(&self, id: SessionId) -> Result<(), SessionRegistryError> {
        let mut guard = self.inner.write();
        if guard.sessions.len() <= 1 {
            return Err(SessionRegistryError::LastSession);
        }
        let Some(idx) = guard.sessions.iter().position(|s| s.id == id) else {
            return Err(SessionRegistryError::NotFound);
        };
        guard.sessions.remove(idx);
        if guard.active == Some(id) {
            guard.active = guard.sessions.first().map(|s| s.id);
        }
        Ok(())
    }

    pub fn set_active(&self, id: SessionId) -> Result<(), SessionRegistryError> {
        let mut guard = self.inner.write();
        if !guard.sessions.iter().any(|s| s.id == id) {
            return Err(SessionRegistryError::NotFound);
        }
        guard.active = Some(id);
        if let Some(session) = guard.sessions.iter_mut().find(|s| s.id == id) {
            session.touch();
        }
        Ok(())
    }

    pub fn rename(&self, id: SessionId, new_name: String) -> Result<(), SessionRegistryError> {
        let mut guard = self.inner.write();
        if guard
            .sessions
            .iter()
            .any(|s| s.name == new_name && s.id != id)
        {
            return Err(SessionRegistryError::DuplicateName);
        }
        let Some(session) = guard.sessions.iter_mut().find(|s| s.id == id) else {
            return Err(SessionRegistryError::NotFound);
        };
        session.name = new_name;
        Ok(())
    }

    pub fn snapshot(&self) -> PersistedSnapshot {
        let guard = self.inner.read();
        PersistedSnapshot {
            payload: serde_json::to_string(&*guard).unwrap_or_else(|err| {
                log::error!("failed to serialize session registry: {err:?}");
                String::from("{}")
            }),
        }
    }

    fn load_from(&self, payload: &str) -> Result<()> {
        let data: PersistedRegistry =
            serde_json::from_str(payload).context("parsing persisted session registry")?;
        *self.inner.write() = data;
        Ok(())
    }
}

pub struct PersistedSnapshot {
    payload: String,
}

impl PersistedSnapshot {
    pub async fn write(self) -> Result<()> {
        GlobalKeyValueStore::global()
            .write_kvp(KVP_KEY.to_owned(), self.payload)
            .await
            .context("writing session registry to KVP")
    }
}

/// Debounced + immediate writer for the session registry.
///
/// `c-defer-persist`: rapid `prefix Tab` window-cycle previously queued
/// a JSON serialization on the background executor on every switch.
/// `mark_dirty` instead arms a 2 s debounce — additional `mark_dirty`
/// calls inside that window coalesce into a single eventual flush.
/// Lifecycle events that need a synchronous on-disk view
/// (attach/detach/create/delete/rename/shutdown) call `flush_now`,
/// which spawns immediately and returns the `Task` so the caller can
/// detach or await as appropriate.
///
/// The pending-task `Mutex` is intentionally coarse: serialization
/// already runs off the foreground thread, and the scheduler itself is
/// only contended at switch rate (a few hundred Hz at most). A
/// finer-grained lock-free design would not pay for itself.
pub struct PersistScheduler {
    dirty: AtomicBool,
    pending_timer: Mutex<Option<Task<()>>>,
    /// Counts the total number of background persist tasks spawned by
    /// this scheduler. Read by `c-switch-budget-harness` tests to
    /// assert that rapid switches coalesce into a single flush.
    flush_count: AtomicU32,
}

use std::sync::atomic::AtomicU32;

const PERSIST_DEBOUNCE: Duration = Duration::from_secs(2);

impl PersistScheduler {
    fn new() -> Self {
        Self {
            dirty: AtomicBool::new(false),
            pending_timer: Mutex::new(None),
            flush_count: AtomicU32::new(0),
        }
    }

    /// Total background persist tasks spawned since the scheduler was
    /// created. Inclusive of both debounced and `flush_now` paths.
    pub fn flush_count(&self) -> u32 {
        self.flush_count.load(Ordering::Relaxed)
    }

    /// Mark the registry dirty. If no flush is currently pending, arm
    /// the debounce timer; otherwise the in-flight timer absorbs this
    /// switch into the eventual single flush.
    pub fn mark_dirty(self: &Arc<Self>, cx: &App) {
        self.dirty.store(true, Ordering::Release);
        let mut guard = self.pending_timer.lock();
        if guard.is_some() {
            // A debounce timer is already armed; coalesce.
            return;
        }
        let scheduler = self.clone();
        let task = cx.spawn(async move |cx| {
            cx.background_executor().timer(PERSIST_DEBOUNCE).await;
            // Clear the pending-timer slot BEFORE spawning the write —
            // the next `mark_dirty` after this point arms a fresh
            // debounce window rather than getting silently swallowed.
            scheduler.pending_timer.lock().take();
            if !scheduler.dirty.swap(false, Ordering::AcqRel) {
                return;
            }
            scheduler.spawn_flush_task(cx).detach();
        });
        *guard = Some(task);
    }

    /// Flush the registry now. The returned task spawns immediately on
    /// the background executor; the caller picks whether to detach or
    /// await. Lifecycle events (attach/detach/create/delete/rename/
    /// shutdown) call this so the on-disk view is consistent at the
    /// boundaries that matter for crash recovery.
    pub fn flush_now(self: &Arc<Self>, cx: &App) -> Task<()> {
        // Any in-flight debounce becomes redundant once we flush
        // synchronously — drop it so the queued task doesn't fire a
        // duplicate write a second later.
        self.pending_timer.lock().take();
        self.dirty.store(false, Ordering::Release);
        // Snapshot via the `&App` we already hold instead of round-
        // tripping through `AsyncApp::update`. The Async path re-borrows
        // the App cell and panics when `flush_now` is called from
        // inside an entity update (window-close handler's `update_in`
        // is the path that hit this).
        self.flush_count.fetch_add(1, Ordering::Relaxed);
        let snapshot = SessionRegistry::global(cx).snapshot();
        cx.background_spawn(async move {
            if let Err(err) = snapshot.write().await {
                log::warn!("persist scheduler: failed to persist session registry: {err:?}");
            }
        })
    }

    fn spawn_flush_task(&self, cx: &mut gpui::AsyncApp) -> Task<()> {
        self.flush_count.fetch_add(1, Ordering::Relaxed);
        let snapshot = cx.update(|cx| SessionRegistry::global(cx).snapshot());
        cx.background_spawn(async move {
            if let Err(err) = snapshot.write().await {
                log::warn!("persist scheduler: failed to persist session registry: {err:?}");
            }
        })
    }
}

#[derive(Clone)]
pub struct PersistSchedulerHandle(Arc<PersistScheduler>);

impl PersistSchedulerHandle {
    pub fn mark_dirty(&self, cx: &App) {
        self.0.mark_dirty(cx);
    }

    pub fn flush_now(&self, cx: &App) -> Task<()> {
        self.0.flush_now(cx)
    }

    pub fn flush_count(&self) -> u32 {
        self.0.flush_count()
    }
}

impl Global for PersistSchedulerHandle {}

/// Cheap (Arc-cloned) global handle. Cloning detaches from the `App`
/// borrow so callers can hold it across `cx.notify()`, switches, etc.
pub fn persist_scheduler(cx: &App) -> PersistSchedulerHandle {
    cx.global::<PersistSchedulerHandle>().clone()
}

pub fn init(cx: &mut App) {
    let registry = SessionRegistry::default();
    match GlobalKeyValueStore::global().read_kvp(KVP_KEY) {
        Ok(Some(payload)) => {
            if let Err(err) = registry.load_from(&payload) {
                log::warn!("failed to load session registry from KVP: {err:?}");
            }
        }
        Ok(None) => {}
        Err(err) => {
            log::warn!("failed to read session registry from KVP: {err:?}");
        }
    }
    cx.set_global(registry);
    cx.set_global(PersistSchedulerHandle(Arc::new(PersistScheduler::new())));

    spawn_heartbeat(cx);

    cx.on_app_quit(|cx| {
        // Shutdown drain: flush the registry synchronously through the
        // scheduler so any pending debounced switch lands on disk
        // before the app exits.
        let snapshot = SessionRegistry::global(cx).snapshot();
        async move {
            if let Err(err) = snapshot.write().await {
                log::warn!("failed to persist session registry on quit: {err:?}");
            }
        }
    })
    .detach();
}

fn spawn_heartbeat(cx: &App) {
    let interval = std::time::Duration::from_secs(30);
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(interval).await;
            let snapshot = cx.update(|cx| SessionRegistry::global(cx).snapshot());
            if let Err(err) = snapshot.write().await {
                log::warn!("session registry heartbeat persist failed: {err:?}");
            }
        }
    })
    .detach();
}
