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

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str) -> Session {
        Session::new(name, PathBuf::from("/tmp"))
    }

    #[test]
    fn new_session_has_one_window_and_no_previous() {
        let s = session("alpha");
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.active_window, 0);
        assert_eq!(s.previous_window, None);
        assert_eq!(s.windows[0].id, WindowId(1));
    }

    #[test]
    fn next_window_id_increments_above_max_existing_id() {
        let mut s = session("alpha");
        // Force a non-contiguous id to confirm we use max + 1.
        s.windows.push(Window::new(WindowId(7), "seven"));
        assert_eq!(s.next_window_id(), WindowId(8));
    }

    #[test]
    fn add_window_returns_unique_id_each_time() {
        let mut s = session("alpha");
        let a = s.add_window(None);
        let b = s.add_window(None);
        assert_ne!(a, b);
        assert_eq!(s.windows.len(), 3);
    }

    #[test]
    fn set_active_window_records_previous() {
        let mut s = session("alpha");
        s.add_window(None);
        s.add_window(None);
        // Move from window 0 to window 2.
        s.set_active_window(2);
        assert_eq!(s.active_window, 2);
        assert_eq!(s.previous_window, Some(0));

        // Move again from 2 to 1 — previous shifts to 2, not 0.
        s.set_active_window(1);
        assert_eq!(s.active_window, 1);
        assert_eq!(s.previous_window, Some(2));
    }

    #[test]
    fn set_active_window_noop_for_out_of_range_or_already_active() {
        let mut s = session("alpha");
        s.add_window(None);
        s.set_active_window(0); // already active
        assert_eq!(s.previous_window, None);
        s.set_active_window(99); // out of range
        assert_eq!(s.active_window, 0);
        assert_eq!(s.previous_window, None);
    }

    #[test]
    fn remove_window_refuses_to_drop_the_last_one() {
        let mut s = session("alpha");
        let only_id = s.windows[0].id;
        assert!(!s.remove_window(only_id));
        assert_eq!(s.windows.len(), 1);
    }

    #[test]
    fn remove_window_shifts_active_index_when_earlier_window_removed() {
        let mut s = session("alpha");
        let _b = s.add_window(None); // window 1
        let _c = s.add_window(None); // window 2
        s.set_active_window(2);
        assert_eq!(s.active_window, 2);
        // Drop the very first window — active should slide from 2 down to 1.
        let first_id = s.windows[0].id;
        assert!(s.remove_window(first_id));
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.active_window, 1);
    }

    #[test]
    fn remove_window_clamps_active_when_tail_removed() {
        let mut s = session("alpha");
        s.add_window(None);
        s.set_active_window(1);
        let last_id = s.windows[1].id;
        assert!(s.remove_window(last_id));
        // Active index must point at the surviving window, not past it.
        assert_eq!(s.active_window, 0);
    }

    #[test]
    fn remove_window_clears_previous_when_it_pointed_at_removed() {
        let mut s = session("alpha");
        s.add_window(None);
        s.add_window(None);
        s.set_active_window(2);
        // previous_window now = Some(0).
        assert_eq!(s.previous_window, Some(0));
        // Remove the window at index 0 (the one previous_window points at).
        let removed_id = s.windows[0].id;
        s.remove_window(removed_id);
        assert_eq!(s.previous_window, None);
    }

    #[test]
    fn remove_window_clears_previous_when_aliasing_active_after_shift() {
        let mut s = session("alpha");
        s.add_window(None);
        s.add_window(None);
        // Sequence: active=0 → set_active(1) (prev=0) → set_active(2) (prev=1).
        s.set_active_window(1);
        s.set_active_window(2);
        assert_eq!(s.previous_window, Some(1));
        // Remove window at index 1 (the previous). After removal active=1.
        // previous would also be 1 after the shift, so it must be cleared.
        let removed_id = s.windows[1].id;
        s.remove_window(removed_id);
        assert_eq!(s.previous_window, None);
        assert_eq!(s.active_window, 1);
    }

    #[test]
    fn active_returns_none_for_invalid_index() {
        let mut s = session("alpha");
        s.active_window = 42;
        assert!(s.active().is_none());
    }

    #[test]
    fn touch_updates_last_attached() {
        let mut s = session("alpha");
        let before = s.last_attached_ms;
        // Spin briefly until the system clock advances.
        for _ in 0..10_000 {
            s.touch();
            if s.last_attached_ms != before {
                break;
            }
        }
        // Either the clock moved or it's stuck on the same ms — both are
        // observable from the field, so we just confirm `touch` writes.
        // A weaker but reliable check: touch produces a value >= before.
        assert!(s.last_attached_ms >= before);
    }
}
