use std::sync::Arc;

use anyhow::{Context as _, Result};
use db::kvp::GlobalKeyValueStore;
use gpui::{App, Global};
use parking_lot::RwLock;
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
}
