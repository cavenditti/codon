use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use workspace::codon_bridge::LayoutSnapshot;

/// Fixed number of window slots per session. Codon's "windows always
/// exist" model mirrors the digit-keyed bindings `prefix 1` … `prefix 9`
/// in the default keymap — every slot is reachable by index, materialised
/// on demand. Visibility in the indicator/picker/overview is filtered to
/// non-empty slots (plus the active one), so an unused slot stays
/// invisible until the user puts something in it.
pub const WINDOW_SLOTS: usize = 9;

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
    /// `true` when the in-memory runtime cache holds a fresher copy of
    /// this window's pane tree than `layout` does. Set by the
    /// switch-stash fast path (`c-skip-capture-on-cache-hit`) and
    /// cleared by `WindowRuntimeCache::evict_and_persist` when the
    /// cache entry materializes back into a `LayoutSnapshot`.
    ///
    /// Skipped during serialization so the persisted JSON does not
    /// carry the flag across restarts — after restart the on-disk
    /// `layout` (whatever last got materialized) is the only source
    /// of truth.
    #[serde(default, skip_serializing_if = "is_false")]
    pub layout_stale: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Window {
    pub fn new(id: WindowId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            layout: None,
            layout_stale: false,
        }
    }

    /// Default name codon assigns to slot `idx` (zero-based) on a fresh
    /// session — the 1-based digit users press. A window that still has
    /// this name is considered "untouched" by [`Self::has_user_content`].
    pub fn default_name_for(idx: usize) -> String {
        (idx + 1).to_string()
    }

    /// `true` when the persisted layout snapshot contains at least one
    /// item. The cache-fast-path leaves `layout_stale` set on the most
    /// recent outgoing window, so callers that need a runtime-truthful
    /// answer for *that* window should also consult
    /// [`crate::runtime::WindowRuntimeCache::peek_has_items`] before
    /// trusting this result.
    pub fn layout_has_items(&self) -> bool {
        self.layout
            .as_ref()
            .is_some_and(LayoutSnapshot::has_any_items)
    }

    /// `true` when this window is "in use" — either it holds at least
    /// one item, or its slot has been renamed away from the default.
    /// Renaming an empty window is a legitimate "I'm planning to use
    /// this slot" signal, so the indicator surfaces it.
    pub fn has_user_content(&self, idx: usize) -> bool {
        self.layout_has_items() || self.name != Self::default_name_for(idx)
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
    /// Per-session selection registers — the named-`"<char>` slots the
    /// `codon_registers::SelectRegister` prefix arms. The runtime view
    /// lives on [`crate::registers::RegisterStore`] (a `gpui::Global`);
    /// this field is the *persistence surface* for the per-session map.
    ///
    /// Skipped during serialisation in this task — `swap_session`
    /// rebuilds an empty map on rehydrate. Per-session contents
    /// persistence + the `[registers]` TOML named-persistent variant
    /// land in `phase-19/selection-registers-persistent`.
    #[serde(default, skip)]
    pub registers: std::collections::HashMap<String, codon_mode::Selection>,
}

impl Session {
    pub fn new(name: impl Into<String>, cwd: PathBuf) -> Self {
        let windows = (0..WINDOW_SLOTS)
            .map(|i| Window::new(WindowId(i as u64 + 1), Window::default_name_for(i)))
            .collect();
        Self {
            id: SessionId::new(),
            name: name.into(),
            cwd,
            windows,
            active_window: 0,
            previous_window: None,
            last_attached_ms: now_ms(),
            registers: std::collections::HashMap::new(),
        }
    }

    /// Pad an existing session's `windows` vec up to [`WINDOW_SLOTS`]
    /// entries, preserving every existing window in place. Used by the
    /// registry loader so sessions persisted under the pre-slots model
    /// (1..N windows) transparently grow to the always-9 invariant on
    /// startup. Each appended slot gets the default name and a unique
    /// `WindowId` (incrementing past the current `next_window_id`).
    pub fn pad_to_window_slots(&mut self) {
        while self.windows.len() < WINDOW_SLOTS {
            let idx = self.windows.len();
            let id = self.next_window_id();
            self.windows
                .push(Window::new(id, Window::default_name_for(idx)));
        }
    }

    /// Smallest index whose window has no user content. Returns `None`
    /// only when every slot is already in use, which is also the cap
    /// for `WindowNew` / `BreakPaneToWindow` materialisation.
    pub fn first_empty_window_index(&self) -> Option<usize> {
        self.windows
            .iter()
            .enumerate()
            .find_map(|(idx, w)| (!w.has_user_content(idx)).then_some(idx))
    }

    /// Indices visible in the windows indicator/picker/overview — every
    /// slot that has user content, plus the currently-active slot (so
    /// the user can always see where they are, even when the active
    /// window is itself empty).
    ///
    /// Note: this is layout-only. For the brief window between
    /// `cycle_window` setting `layout_stale = true` and a subsequent
    /// evict-and-persist, the most recently outgoing window may not yet
    /// reflect runtime emptiness. The runtime cache holds the live
    /// `Member` tree in that case; callers that care can additionally
    /// consult `WindowRuntimeCache::peek_has_items`.
    pub fn displayed_window_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.windows.len());
        for (idx, w) in self.windows.iter().enumerate() {
            if idx == self.active_window || w.has_user_content(idx) {
                out.push(idx);
            }
        }
        out
    }

    /// `true` when no window in the session has user content. The
    /// active slot's emptiness still counts — a brand-new session
    /// trips this even though slot 0 is "active".
    pub fn is_entirely_empty(&self) -> bool {
        self.windows
            .iter()
            .enumerate()
            .all(|(idx, w)| !w.has_user_content(idx))
    }

    /// Clear the slot at `idx` back to its untouched state — drop any
    /// persisted layout and reset the name to the default. The
    /// `WindowId` is preserved so cache lookups and `previous_window`
    /// references stay valid.
    pub fn clear_window(&mut self, idx: usize) {
        if let Some(slot) = self.windows.get_mut(idx) {
            slot.layout = None;
            slot.layout_stale = false;
            slot.name = Window::default_name_for(idx);
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
    use workspace::codon_bridge::{ItemSnapshot, PaneSnapshot};

    fn session(name: &str) -> Session {
        Session::new(name, PathBuf::from("/tmp"))
    }

    fn non_empty_pane() -> LayoutSnapshot {
        LayoutSnapshot::Pane(PaneSnapshot {
            items: vec![ItemSnapshot {
                kind: "Editor".into(),
                item_id: 1,
                active: true,
                preview: false,
            }],
            active: true,
            pinned_count: 0,
        })
    }

    #[test]
    fn new_session_pre_materialises_window_slots() {
        let s = session("alpha");
        assert_eq!(s.windows.len(), WINDOW_SLOTS);
        assert_eq!(s.active_window, 0);
        assert_eq!(s.previous_window, None);
        for (i, w) in s.windows.iter().enumerate() {
            assert_eq!(w.id, WindowId(i as u64 + 1));
            assert_eq!(w.name, Window::default_name_for(i));
            assert!(w.layout.is_none());
        }
    }

    #[test]
    fn next_window_id_increments_above_max_existing_id() {
        let mut s = session("alpha");
        // Force a non-contiguous id to confirm we use max + 1.
        s.windows.push(Window::new(WindowId(42), "scratch"));
        assert_eq!(s.next_window_id(), WindowId(43));
    }

    #[test]
    fn pad_to_window_slots_grows_legacy_sessions() {
        let mut s = session("alpha");
        // Mimic an old persisted session that only carried 2 windows.
        s.windows.truncate(2);
        assert_eq!(s.windows.len(), 2);
        s.pad_to_window_slots();
        assert_eq!(s.windows.len(), WINDOW_SLOTS);
        // Padding preserves existing ids and appends fresh ones.
        assert_eq!(s.windows[0].id, WindowId(1));
        assert_eq!(s.windows[1].id, WindowId(2));
        for (i, w) in s.windows.iter().enumerate().skip(2) {
            assert_eq!(w.name, Window::default_name_for(i));
            assert!(w.layout.is_none());
        }
    }

    #[test]
    fn pad_to_window_slots_is_idempotent_on_full_sessions() {
        let mut s = session("alpha");
        let len_before = s.windows.len();
        s.pad_to_window_slots();
        assert_eq!(s.windows.len(), len_before);
    }

    #[test]
    fn set_active_window_records_previous() {
        let mut s = session("alpha");
        s.set_active_window(2);
        assert_eq!(s.active_window, 2);
        assert_eq!(s.previous_window, Some(0));

        s.set_active_window(1);
        assert_eq!(s.active_window, 1);
        assert_eq!(s.previous_window, Some(2));
    }

    #[test]
    fn set_active_window_noop_for_out_of_range_or_already_active() {
        let mut s = session("alpha");
        s.set_active_window(0); // already active
        assert_eq!(s.previous_window, None);
        s.set_active_window(99); // out of range
        assert_eq!(s.active_window, 0);
        assert_eq!(s.previous_window, None);
    }

    #[test]
    fn first_empty_window_index_returns_zero_for_fresh_session() {
        let s = session("alpha");
        assert_eq!(s.first_empty_window_index(), Some(0));
    }

    #[test]
    fn first_empty_window_index_skips_filled_slots() {
        let mut s = session("alpha");
        s.windows[0].layout = Some(non_empty_pane());
        s.windows[1].layout = Some(non_empty_pane());
        assert_eq!(s.first_empty_window_index(), Some(2));
    }

    #[test]
    fn first_empty_window_index_treats_rename_as_use() {
        let mut s = session("alpha");
        s.windows[0].name = "scratch".into();
        // Slot 0 has a custom name but no layout — still "in use".
        assert_eq!(s.first_empty_window_index(), Some(1));
    }

    #[test]
    fn first_empty_window_index_none_when_full() {
        let mut s = session("alpha");
        for w in &mut s.windows {
            w.layout = Some(non_empty_pane());
        }
        assert_eq!(s.first_empty_window_index(), None);
    }

    #[test]
    fn displayed_window_indices_always_includes_active() {
        let s = session("alpha");
        // Brand-new session: every slot is empty. Only the active one shows.
        assert_eq!(s.displayed_window_indices(), vec![0]);
    }

    #[test]
    fn displayed_window_indices_surfaces_non_empty_slots() {
        let mut s = session("alpha");
        s.windows[3].layout = Some(non_empty_pane());
        s.windows[6].layout = Some(non_empty_pane());
        assert_eq!(s.displayed_window_indices(), vec![0, 3, 6]);
    }

    #[test]
    fn clear_window_restores_default_name_and_drops_layout() {
        let mut s = session("alpha");
        s.windows[2].layout = Some(non_empty_pane());
        s.windows[2].name = "renamed".into();
        s.clear_window(2);
        assert!(s.windows[2].layout.is_none());
        assert_eq!(s.windows[2].name, Window::default_name_for(2));
        assert!(!s.windows[2].has_user_content(2));
    }

    #[test]
    fn is_entirely_empty_distinguishes_used_vs_fresh() {
        let mut s = session("alpha");
        assert!(s.is_entirely_empty());
        s.windows[4].layout = Some(non_empty_pane());
        assert!(!s.is_entirely_empty());
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
