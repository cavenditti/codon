//! Tmux-style nested overview of sessions and their windows.
//!
//! Replaces the earlier tile-grid `SessionOverviewModal` and
//! `WindowOverviewModal` with a single tree view. Each session is a
//! top-level row; its windows are children. Keyboard navigation walks
//! the flattened set of visible rows.
//!
//! The active window's layout is re-captured from the live workspace on
//! open so the pane count and layout shorthand reflect current state,
//! not whatever was snapshotted on the previous window-switch.

use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement, Render, Styled, Window,
};
use ui::{
    ActiveTheme as _, Color, FluentBuilder as _, Label, LabelCommon as _, LabelSize, StyledExt as _,
    div, h_flex, v_flex,
};
use workspace::{
    DismissDecision, ModalView, Workspace,
    codon_bridge::{LayoutSnapshot, SnapshotAxis},
};

use crate::{
    registry::SessionRegistry,
    session::{Session, SessionId, WindowId},
    window_indicator::{commit_active_window, preview_switch_to_window, switch_to_window},
};

const MODAL_W_FRAC: f32 = 0.6;
const MODAL_H_FRAC: f32 = 0.7;

/// Selected starting row when the modal opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialFocus {
    /// Land on the active session's session row (used by `SessionOverview`).
    Session,
    /// Land on the active window's window row (used by `WindowOverview`).
    Window,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Session { session: usize },
    Window { session: usize, window: usize },
}

pub struct OverviewModal {
    workspace: gpui::WeakEntity<Workspace>,
    focus: FocusHandle,
    /// Snapshot of sessions taken at modal-open. The active session's
    /// active window has its `layout` overwritten with a freshly
    /// captured snapshot so the pane count/shorthand are accurate.
    sessions: Vec<Session>,
    active_session_id: Option<SessionId>,
    /// Window the workspace was on when the modal opened. Used to
    /// restore on Esc/dismiss after a backdrop preview.
    origin_window_id: Option<WindowId>,
    /// Window currently visible behind the modal due to a preview swap.
    /// `None` means the workspace still shows `origin_window_id`.
    previewed_window_id: Option<WindowId>,
    /// Per-session expansion flag, parallel to `sessions`.
    expanded: Vec<bool>,
    /// Flattened list of currently-visible rows. Rebuilt whenever
    /// `expanded` changes.
    rows: Vec<Row>,
    selected: usize,
}

impl OverviewModal {
    pub fn new(
        focus: InitialFocus,
        workspace: gpui::WeakEntity<Workspace>,
        live_snapshot: Option<LayoutSnapshot>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let registry = SessionRegistry::global(cx);
        let mut sessions = registry.sessions();
        sessions.sort_by(|a, b| b.last_attached_ms.cmp(&a.last_attached_ms));
        let active_session_id = registry.active_id();

        // The active session's active-window `layout` may be stale: the
        // switch fast path (`c-skip-capture-on-cache-hit`) keeps the
        // freshest copy in the runtime cache rather than re-materializing
        // a snapshot on every switch. `c-overview-defer-capture` lets the
        // modal open without paying the synchronous capture cost — the
        // persisted snapshot (last materialized at eviction/detach/
        // shutdown) is good enough for the pane-count / shorthand
        // summary. Callers that want a live overlay can still pass an
        // already-captured `LayoutSnapshot` and it splices in here.
        if let (Some(active_id), Some(snapshot)) = (active_session_id, live_snapshot)
            && let Some(session) = sessions.iter_mut().find(|s| s.id == active_id)
            && let Some(active_window) = session.active_mut()
        {
            active_window.layout = Some(snapshot);
        }

        let expanded = vec![true; sessions.len()];
        let rows = build_rows(&sessions, &expanded);
        let selected = pick_initial(&rows, &sessions, active_session_id, focus);
        let origin_window_id = active_session_id
            .and_then(|id| sessions.iter().find(|s| s.id == id))
            .and_then(|s| s.active().map(|w| w.id));

        Self {
            workspace,
            focus: cx.focus_handle(),
            sessions,
            active_session_id,
            origin_window_id,
            previewed_window_id: None,
            expanded,
            rows,
            selected,
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.rows.is_empty() {
            if event.keystroke.key == "escape" {
                cx.emit(DismissEvent);
            }
            return;
        }

        let key = event.keystroke.key.as_str();
        let mut handled = true;

        match key {
            "j" | "down" => {
                if self.selected + 1 < self.rows.len() {
                    self.selected += 1;
                }
            }
            "k" | "up" => {
                self.selected = self.selected.saturating_sub(1);
            }
            "h" | "left" => self.collapse_or_ascend(),
            "l" | "right" => self.expand_or_descend(),
            "g" => self.selected = 0,
            "shift-g" => self.selected = self.rows.len() - 1,
            "enter" => {
                self.activate(window, cx);
                return;
            }
            "escape" => {
                cx.emit(DismissEvent);
                return;
            }
            _ => handled = false,
        }

        if handled {
            self.update_preview(window, cx);
            cx.notify();
        }
    }

    /// The window the workspace *should* be showing for the current
    /// selection. `None` means "show origin" — same-session session
    /// rows and cross-session rows both fall back to origin so that
    /// no cross-session attach happens during navigation.
    fn desired_preview(&self) -> Option<WindowId> {
        let Row::Window { session, window } = *self.rows.get(self.selected)? else {
            return None;
        };
        let s = self.sessions.get(session)?;
        if Some(s.id) != self.active_session_id {
            return None;
        }
        s.windows.get(window).map(|w| w.id)
    }

    /// Push the workspace's center group toward whatever
    /// `desired_preview` resolves to, swapping back to origin when
    /// it returns `None`. No-op if the workspace is already on the
    /// right window.
    fn update_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session_id) = self.active_session_id else {
            return;
        };
        let Some(origin) = self.origin_window_id else {
            return;
        };
        let target = self.desired_preview().unwrap_or(origin);
        let current = self.previewed_window_id.unwrap_or(origin);
        if target == current {
            return;
        }

        let target_layout = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.windows.iter().find(|w| w.id == target))
            .and_then(|w| w.layout.clone());

        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let focus = self.focus.clone();
        workspace.update(cx, |workspace, cx| {
            preview_switch_to_window(
                workspace,
                session_id,
                current,
                target,
                target_layout,
                focus,
                window,
                cx,
            );
        });

        self.previewed_window_id = if target == origin { None } else { Some(target) };
    }

    /// Restore the workspace to `origin_window_id`. Called before
    /// dismissing so that Esc/click-outside doesn't leave the user on
    /// a previewed window they never explicitly chose.
    fn restore_origin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session_id) = self.active_session_id else {
            return;
        };
        let Some(origin) = self.origin_window_id else {
            return;
        };
        let Some(current) = self.previewed_window_id else {
            return;
        };

        let target_layout = self
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|s| s.windows.iter().find(|w| w.id == origin))
            .and_then(|w| w.layout.clone());

        if let Some(workspace) = self.workspace.upgrade() {
            let focus = self.focus.clone();
            workspace.update(cx, |workspace, cx| {
                preview_switch_to_window(
                    workspace,
                    session_id,
                    current,
                    origin,
                    target_layout,
                    focus,
                    window,
                    cx,
                );
            });
        }
        self.previewed_window_id = None;
    }

    /// `h`: if on a window row, jump to the parent session row. If on
    /// a session row that's expanded, collapse it. Already collapsed
    /// session rows are a no-op (matches tmux's `prefix s`).
    fn collapse_or_ascend(&mut self) {
        match self.rows[self.selected] {
            Row::Window { session, .. } => {
                if let Some(parent) = self
                    .rows
                    .iter()
                    .position(|r| matches!(r, Row::Session { session: s } if *s == session))
                {
                    self.selected = parent;
                }
            }
            Row::Session { session } => {
                if self.expanded[session] {
                    self.expanded[session] = false;
                    self.rebuild_rows_preserving_selection(Row::Session { session });
                }
            }
        }
    }

    /// `l`: if on a session row that's collapsed, expand it. If on a
    /// session row that's already expanded, drop into its first
    /// window. On a window row, no-op.
    fn expand_or_descend(&mut self) {
        match self.rows[self.selected] {
            Row::Session { session } => {
                if !self.expanded[session] {
                    self.expanded[session] = true;
                    self.rebuild_rows_preserving_selection(Row::Session { session });
                } else if let Some(first_child) = self
                    .rows
                    .iter()
                    .position(|r| matches!(r, Row::Window { session: s, .. } if *s == session))
                {
                    self.selected = first_child;
                }
            }
            Row::Window { .. } => {}
        }
    }

    fn rebuild_rows_preserving_selection(&mut self, anchor: Row) {
        self.rows = build_rows(&self.sessions, &self.expanded);
        self.selected = self
            .rows
            .iter()
            .position(|r| *r == anchor)
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(self.selected).copied() else {
            self.restore_origin(window, cx);
            cx.emit(DismissEvent);
            return;
        };
        let registry = SessionRegistry::global(cx);
        match row {
            Row::Session { session } => {
                let Some(target_id) = self.sessions.get(session).map(|s| s.id) else {
                    self.restore_origin(window, cx);
                    cx.emit(DismissEvent);
                    return;
                };
                let same_session = Some(target_id) == self.active_session_id;
                // Restore origin first so cross-session attach captures
                // the right outgoing state, and same-session no-op
                // doesn't leave us on a previewed window.
                self.restore_origin(window, cx);
                if same_session {
                    cx.emit(DismissEvent);
                } else {
                    self.attach_session(target_id, None, &registry, window, cx);
                }
            }
            Row::Window { session, window: w } => {
                let resolved = self
                    .sessions
                    .get(session)
                    .and_then(|s| s.windows.get(w).map(|win| (s.id, win.id)));
                let Some((session_id, window_id)) = resolved else {
                    self.restore_origin(window, cx);
                    cx.emit(DismissEvent);
                    return;
                };
                let same_session = Some(session_id) == self.active_session_id;
                if same_session {
                    // Preview already swapped the workspace to `window_id`.
                    // Anchoring against the preview state, commit the
                    // session's active_window so registry/persist match
                    // what the user sees. No workspace swap needed.
                    if Some(window_id) != self.origin_window_id {
                        // Fallback for the case where preview was skipped
                        // (shouldn't happen — defensive).
                        if Some(window_id) != self.previewed_window_id
                            && let Some(workspace) = self.workspace.upgrade()
                        {
                            workspace.update(cx, |workspace, cx| {
                                switch_to_window(workspace, window_id, window, cx);
                            });
                        } else {
                            commit_active_window(w, cx);
                        }
                    }
                    self.previewed_window_id = None;
                    cx.emit(DismissEvent);
                } else {
                    self.restore_origin(window, cx);
                    self.attach_session(session_id, Some(w), &registry, window, cx);
                }
            }
        }
    }

    fn attach_session(
        &mut self,
        id: SessionId,
        pin_active_window: Option<usize>,
        registry: &SessionRegistry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = pin_active_window
            && let Some(mut session) = registry.get(id)
        {
            if idx < session.windows.len() && session.active_window != idx {
                session.active_window = idx;
                if let Err(err) = registry.upsert(session) {
                    log::warn!("could not pin active window before attach: {err:?}");
                }
            }
        }
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                crate::actions::attach_session(workspace, id, window, cx);
            });
        }
        cx.emit(DismissEvent);
    }
}

impl EventEmitter<DismissEvent> for OverviewModal {}
impl ModalView for OverviewModal {
    fn on_before_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DismissDecision {
        // Click-outside / focus-loss path: always restore the origin so
        // the user never lands on a previewed window they didn't pick.
        self.restore_origin(window, cx);
        DismissDecision::Dismiss(true)
    }
}

impl Focusable for OverviewModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for OverviewModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let viewport = window.viewport_size();
        let max_w = gpui::px((f32::from(viewport.width) * MODAL_W_FRAC).min(900.0));
        let max_h = gpui::px(f32::from(viewport.height) * MODAL_H_FRAC);

        let header = h_flex()
            .w_full()
            .justify_between()
            .px_1()
            .pb_2()
            .child(Label::new("Overview").size(LabelSize::Large))
            .child(
                Label::new("j/k move · h/l collapse/expand · Enter attach · Esc dismiss")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );

        let body = if self.rows.is_empty() {
            div()
                .p_8()
                .child(Label::new("No sessions yet — `cmd-k s n` to create one.").color(Color::Muted))
                .into_any_element()
        } else {
            let mut list = v_flex().gap_0();
            for (ix, row) in self.rows.iter().enumerate() {
                list = list.child(self.render_row(ix, *row, cx));
            }
            list.into_any_element()
        };

        v_flex()
            .key_context("CodonOverview")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::handle_key_down))
            .elevation_3(cx)
            .max_w(max_w)
            .max_h(max_h)
            .p_4()
            .gap_2()
            .bg(theme.colors().elevated_surface_background)
            .child(header)
            .child(body)
    }
}

impl OverviewModal {
    fn render_row(&self, index: usize, row: Row, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let is_selected = index == self.selected;
        let bg = if is_selected {
            theme.colors().element_selected
        } else {
            theme.colors().elevated_surface_background
        };

        match row {
            Row::Session { session } => {
                let s = &self.sessions[session];
                let is_active = Some(s.id) == self.active_session_id;
                let glyph = if self.expanded[session] { "▼" } else { "▶" };
                let name = s.name.clone();
                let cwd = truncate_middle(&s.cwd.display().to_string(), 38);
                let win_count = s.windows.len();
                let when = format_relative_ms(s.last_attached_ms);

                h_flex()
                    .id(("codon-overview-row", index))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .bg(bg)
                    .child(div().w(gpui::px(14.0)).child(Label::new(glyph).color(Color::Muted)))
                    .child(
                        Label::new(name)
                            .size(LabelSize::Default)
                            .when(is_active, |l| l.color(Color::Accent)),
                    )
                    .child(
                        Label::new(cwd)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(div().flex_grow())
                    .child(
                        Label::new(format!(
                            "{} window{}",
                            win_count,
                            if win_count == 1 { "" } else { "s" }
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    )
                    .child(
                        Label::new(when)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element()
            }
            Row::Window { session, window: w } => {
                let s = &self.sessions[session];
                let win = &s.windows[w];
                let is_active_session = Some(s.id) == self.active_session_id;
                let is_active_window = is_active_session && s.active_window == w;
                let marker = if is_active_window { "●" } else { " " };
                let panes = count_panes(win.layout.as_ref());
                let shorthand = layout_shorthand(win.layout.as_ref());

                h_flex()
                    .id(("codon-overview-row", index))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .bg(bg)
                    .child(div().w(gpui::px(14.0)))
                    .child(
                        div()
                            .w(gpui::px(14.0))
                            .child(Label::new(marker).color(Color::Accent)),
                    )
                    .child(
                        Label::new(format!("{}:", w + 1))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(win.name.clone())
                            .when(is_active_window, |l| l.color(Color::Accent)),
                    )
                    .child(div().flex_grow())
                    .child(
                        Label::new(format!("{} pane{}", panes, if panes == 1 { "" } else { "s" }))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .w(gpui::px(40.0))
                            .child(Label::new(shorthand).size(LabelSize::Small).color(Color::Muted)),
                    )
                    .into_any_element()
            }
        }
    }
}

fn build_rows(sessions: &[Session], expanded: &[bool]) -> Vec<Row> {
    let mut rows = Vec::with_capacity(sessions.len() * 2);
    for (si, session) in sessions.iter().enumerate() {
        rows.push(Row::Session { session: si });
        if expanded.get(si).copied().unwrap_or(false) {
            // Only the slots that have user content (plus the active
            // slot) get a row — keeps the always-9-window model from
            // burying the user under empty entries in the tree view.
            for wi in session.displayed_window_indices() {
                rows.push(Row::Window {
                    session: si,
                    window: wi,
                });
            }
        }
    }
    rows
}

fn pick_initial(
    rows: &[Row],
    sessions: &[Session],
    active_id: Option<SessionId>,
    focus: InitialFocus,
) -> usize {
    let Some(active_id) = active_id else {
        return 0;
    };
    let Some(active_si) = sessions.iter().position(|s| s.id == active_id) else {
        return 0;
    };
    match focus {
        InitialFocus::Session => rows
            .iter()
            .position(|r| matches!(r, Row::Session { session } if *session == active_si))
            .unwrap_or(0),
        InitialFocus::Window => {
            let active_wi = sessions[active_si].active_window;
            rows.iter()
                .position(|r| {
                    matches!(r, Row::Window { session, window } if *session == active_si && *window == active_wi)
                })
                .unwrap_or_else(|| {
                    rows.iter()
                        .position(|r| matches!(r, Row::Session { session } if *session == active_si))
                        .unwrap_or(0)
                })
        }
    }
}

/// Compact one-or-two-char description of the dominant layout axis.
/// Empty for a single pane — at that point the "1 pane" meta column
/// is sufficient and the shorthand would just add noise.
fn layout_shorthand(layout: Option<&LayoutSnapshot>) -> String {
    match layout {
        None | Some(LayoutSnapshot::Pane(_)) => String::new(),
        Some(LayoutSnapshot::Stack { .. }) => "≡".to_string(),
        Some(LayoutSnapshot::Group { axis, children, .. }) => {
            let primary = match axis {
                SnapshotAxis::Horizontal => '|',
                SnapshotAxis::Vertical => '-',
            };
            let nested = children.iter().any(|c| {
                matches!(
                    c,
                    LayoutSnapshot::Group { .. } | LayoutSnapshot::Stack { .. }
                )
            });
            if nested {
                format!("{primary}…")
            } else {
                primary.to_string()
            }
        }
    }
}

fn count_panes(layout: Option<&LayoutSnapshot>) -> usize {
    fn walk(node: &LayoutSnapshot) -> usize {
        match node {
            LayoutSnapshot::Group { children, .. } => children.iter().map(walk).sum(),
            LayoutSnapshot::Stack { members, active } => {
                members.get(*active).map(walk).unwrap_or(1)
            }
            LayoutSnapshot::Pane(_) => 1,
        }
    }
    layout.map(walk).unwrap_or(1)
}

fn truncate_middle(input: &str, max_chars: usize) -> String {
    let len = input.chars().count();
    if len <= max_chars || max_chars < 3 {
        return input.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let head_str: String = input.chars().take(head).collect();
    let tail_str: String = input.chars().skip(len - tail).collect();
    format!("{head_str}…{tail_str}")
}

fn format_relative_ms(then_ms: i64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    if then_ms <= 0 {
        return "—".to_string();
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let delta_secs = ((now_ms - then_ms).max(0)) / 1000;

    if delta_secs < 30 {
        "just now".to_string()
    } else if delta_secs < 60 {
        format!("{delta_secs}s ago")
    } else if delta_secs < 3_600 {
        format!("{}m ago", delta_secs / 60)
    } else if delta_secs < 86_400 {
        format!("{}h ago", delta_secs / 3_600)
    } else if delta_secs < 7 * 86_400 {
        format!("{}d ago", delta_secs / 86_400)
    } else {
        let secs = then_ms / 1000;
        chrono::DateTime::<chrono::Local>::from(
            UNIX_EPOCH + std::time::Duration::from_secs(secs.max(0) as u64),
        )
        .format("%Y-%m-%d")
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use workspace::codon_bridge::{ItemSnapshot, PaneSnapshot};

    fn session_with(name: &str, window_names: &[&str]) -> Session {
        // The always-9-slots invariant means `Session::new` pre-seeds
        // every slot. The fixture marks the first `window_names.len()`
        // slots as user-populated by giving them a non-empty layout
        // (`displayed_window_indices` filters on that) and renames them
        // so the rendered labels match the test expectations.
        let mut s = Session::new(name, PathBuf::from("/tmp"));
        for (idx, label) in window_names.iter().enumerate() {
            let Some(slot) = s.windows.get_mut(idx) else {
                break;
            };
            slot.name = (*label).to_string();
            slot.layout = Some(pane("Editor"));
        }
        s
    }

    fn pane(kind: &str) -> LayoutSnapshot {
        LayoutSnapshot::Pane(PaneSnapshot {
            items: vec![ItemSnapshot {
                kind: kind.to_string(),
                item_id: 1,
                active: true,
                preview: false,
            }],
            active: true,
            pinned_count: 0,
        })
    }

    #[test]
    fn rows_flatten_with_expansion() {
        let sessions = vec![session_with("a", &["1", "2"]), session_with("b", &["1"])];
        let rows = build_rows(&sessions, &[true, false]);
        assert_eq!(rows.len(), 4);
        assert!(matches!(rows[0], Row::Session { session: 0 }));
        assert!(matches!(rows[1], Row::Window { session: 0, window: 0 }));
        assert!(matches!(rows[2], Row::Window { session: 0, window: 1 }));
        assert!(matches!(rows[3], Row::Session { session: 1 }));
    }

    #[test]
    fn collapsed_session_hides_its_windows() {
        let sessions = vec![session_with("a", &["1", "2"])];
        let rows = build_rows(&sessions, &[false]);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn shorthand_empty_for_single_pane() {
        assert_eq!(layout_shorthand(None), "");
        assert_eq!(layout_shorthand(Some(&pane("Terminal"))), "");
    }

    #[test]
    fn shorthand_picks_axis_char() {
        let h = LayoutSnapshot::Group {
            axis: SnapshotAxis::Horizontal,
            flexes: None,
            children: vec![pane("Terminal"), pane("Editor")],
        };
        assert_eq!(layout_shorthand(Some(&h)), "|");

        let v = LayoutSnapshot::Group {
            axis: SnapshotAxis::Vertical,
            flexes: None,
            children: vec![pane("Terminal"), pane("Editor")],
        };
        assert_eq!(layout_shorthand(Some(&v)), "-");
    }

    #[test]
    fn shorthand_marks_nested_groups() {
        let nested = LayoutSnapshot::Group {
            axis: SnapshotAxis::Horizontal,
            flexes: None,
            children: vec![
                pane("Terminal"),
                LayoutSnapshot::Group {
                    axis: SnapshotAxis::Vertical,
                    flexes: None,
                    children: vec![pane("Editor"), pane("Editor")],
                },
            ],
        };
        assert_eq!(layout_shorthand(Some(&nested)), "|…");
    }

    #[test]
    fn count_panes_walks_groups() {
        let layout = LayoutSnapshot::Group {
            axis: SnapshotAxis::Vertical,
            flexes: None,
            children: vec![
                pane("Terminal"),
                LayoutSnapshot::Group {
                    axis: SnapshotAxis::Horizontal,
                    flexes: None,
                    children: vec![pane("Editor"), pane("Editor")],
                },
            ],
        };
        assert_eq!(count_panes(Some(&layout)), 3);
    }

    #[test]
    fn count_panes_empty_layout_is_one() {
        assert_eq!(count_panes(None), 1);
    }

    #[test]
    fn truncate_middle_long_string_keeps_head_and_tail() {
        let path = "/Users/someone/Devel/personal/codon_v3/crates/codon-session";
        let out = truncate_middle(path, 20);
        assert!(out.contains('…'));
        assert!(out.chars().count() <= 20);
    }

    #[test]
    fn format_relative_handles_zero() {
        assert_eq!(format_relative_ms(0), "—");
    }
}
