use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use workspace::codon_bridge::LayoutSnapshot;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub id: WindowId,
    pub name: String,
    pub layout: Option<LayoutSnapshot>,
}

impl Window {
    pub fn new(id: WindowId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            layout: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub cwd: PathBuf,
    pub windows: Vec<Window>,
    pub active_window: usize,
    /// Unix epoch millis. Stored as i64 so the persisted format is portable.
    pub last_attached_ms: i64,
}

impl Session {
    pub fn new(name: impl Into<String>, cwd: PathBuf) -> Self {
        let initial_window = Window::new(WindowId(1), "1");
        Self {
            id: SessionId::new(),
            name: name.into(),
            cwd,
            windows: vec![initial_window],
            active_window: 0,
            last_attached_ms: now_ms(),
        }
    }

    pub fn touch(&mut self) {
        self.last_attached_ms = now_ms();
    }

    pub fn active(&self) -> Option<&Window> {
        self.windows.get(self.active_window)
    }

    pub fn active_mut(&mut self) -> Option<&mut Window> {
        self.windows.get_mut(self.active_window)
    }

    pub fn next_window_id(&self) -> WindowId {
        let max = self.windows.iter().map(|w| w.id.0).max().unwrap_or(0);
        WindowId(max + 1)
    }

    pub fn add_window(&mut self, name: Option<String>) -> WindowId {
        let id = self.next_window_id();
        let name = name.unwrap_or_else(|| format!("{}", id.0));
        self.windows.push(Window::new(id, name));
        id
    }

    pub fn remove_window(&mut self, id: WindowId) -> bool {
        let Some(index) = self.windows.iter().position(|w| w.id == id) else {
            return false;
        };
        if self.windows.len() == 1 {
            return false;
        }
        self.windows.remove(index);
        if self.active_window >= self.windows.len() {
            self.active_window = self.windows.len() - 1;
        } else if self.active_window > index {
            self.active_window -= 1;
        }
        true
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
