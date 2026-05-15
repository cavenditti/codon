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
    /// Index of the previously-active window in `windows`, if any. Used by
    /// `WindowLast` to implement tmux's `prefix l` toggle. Validated at
    /// read time — a stale index (e.g. window removed since this was set)
    /// is treated as `None` by callers.
    #[serde(default)]
    pub previous_window: Option<usize>,
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
            previous_window: None,
            last_attached_ms: now_ms(),
        }
    }

    /// Set the active window index and shift the prior value into
    /// `previous_window`. No-op if `new_active` is already active or
    /// out of range — callers don't want a stale "previous" that
    /// points at the window we just left.
    pub fn set_active_window(&mut self, new_active: usize) {
        if new_active >= self.windows.len() || new_active == self.active_window {
            return;
        }
        self.previous_window = Some(self.active_window);
        self.active_window = new_active;
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
        self.previous_window = match self.previous_window {
            Some(p) if p == index => None,
            Some(p) if p > index => Some(p - 1),
            other => other,
        };
        if self.previous_window == Some(self.active_window) {
            self.previous_window = None;
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
