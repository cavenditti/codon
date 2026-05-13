use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyDownEvent, Keystroke, ParentElement, Render, Styled, Window,
};
use ui::{
    ActiveTheme as _, Color, FluentBuilder as _, Label, LabelCommon as _, LabelSize, StyledExt as _,
    div, h_flex, v_flex,
};
use workspace::{ModalView, Workspace};

use crate::{
    registry::SessionRegistry,
    session::{Session, SessionId},
};

const TILE_WIDTH: f32 = 240.0;
const TILE_HEIGHT: f32 = 110.0;
const TILE_GAP: f32 = 12.0;
const MODAL_PAD: f32 = 32.0;
/// Viewport fraction the overview is allowed to consume.
const MODAL_W_FRAC: f32 = 0.85;
const MODAL_H_FRAC: f32 = 0.85;
/// Used only as a pre-render fallback before `render()` reads the viewport.
const COLUMNS_FALLBACK: usize = 4;

/// Visible columns given a viewport width — clamped so navigation never
/// stalls at < 2 columns and never spreads the grid uncomfortably wide.
fn columns_for_viewport(viewport_w: f32) -> usize {
    let usable = (viewport_w * MODAL_W_FRAC - MODAL_PAD).max(TILE_WIDTH);
    let cols = (usable / (TILE_WIDTH + TILE_GAP)).floor() as usize;
    cols.clamp(2, 6)
}

pub struct SessionOverviewModal {
    workspace: gpui::WeakEntity<Workspace>,
    focus: FocusHandle,
    /// Snapshot of sessions taken at modal-open time. Stable indices keep
    /// keyboard navigation simple — we don't re-sort while open.
    sessions: Vec<Session>,
    selected: usize,
    columns: usize,
}

impl SessionOverviewModal {
    pub fn new(
        workspace: gpui::WeakEntity<Workspace>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let registry = SessionRegistry::global(cx);
        let mut sessions = registry.sessions();
        // Most-recently-attached first so the user's "where was I" lands
        // near the top-left, matching tmux's `prefix s` ordering.
        sessions.sort_by(|a, b| b.last_attached_ms.cmp(&a.last_attached_ms));

        let active_id = registry.active_id();
        let selected = active_id
            .and_then(|id| sessions.iter().position(|s| s.id == id))
            .unwrap_or(0);

        Self {
            workspace,
            focus: cx.focus_handle(),
            sessions,
            selected,
            columns: COLUMNS_FALLBACK,
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sessions.is_empty() {
            if matches_dismiss(&event.keystroke) {
                cx.emit(DismissEvent);
            }
            return;
        }

        let key = event.keystroke.key.as_str();
        let cols = self.columns.max(1);
        let len = self.sessions.len();
        let mut handled = true;

        match key {
            "h" | "left" => {
                if self.selected % cols > 0 {
                    self.selected -= 1;
                }
            }
            "l" | "right" => {
                if self.selected + 1 < len && (self.selected + 1) % cols != 0 {
                    self.selected += 1;
                }
            }
            "k" | "up" => {
                if self.selected >= cols {
                    self.selected -= cols;
                }
            }
            "j" | "down" => {
                if self.selected + cols < len {
                    self.selected += cols;
                }
            }
            "enter" => {
                self.attach_selected(window, cx);
                return;
            }
            "escape" => {
                cx.emit(DismissEvent);
                return;
            }
            _ => handled = false,
        }

        if handled {
            cx.notify();
        }
    }

    fn attach_selected(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get(self.selected) else {
            cx.emit(DismissEvent);
            return;
        };
        let id = session.id;
        if let Err(err) = SessionRegistry::global(cx).set_active(id) {
            log::warn!("could not activate session: {err:?}");
            cx.emit(DismissEvent);
            return;
        }
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.set_session_id(Some(id.to_string()));
                cx.notify();
            });
        }
        cx.emit(DismissEvent);
    }

    fn render_tile(
        &self,
        index: usize,
        session: &Session,
        active_id: Option<SessionId>,
        cx: &App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_active = Some(session.id) == active_id;
        let is_selected = index == self.selected;

        let border_color = if is_active {
            theme.colors().text_accent
        } else if is_selected {
            theme.colors().border_focused
        } else {
            theme.colors().border_variant
        };

        let bg = if is_selected {
            theme.colors().element_selected
        } else {
            theme.colors().elevated_surface_background
        };

        let cwd_display = truncate_middle(&session.cwd.display().to_string(), 36);
        let last_attached = format_relative_ms(session.last_attached_ms);

        v_flex()
            .id(("codon-session-overview-tile", index))
            .w(gpui::px(TILE_WIDTH))
            .h(gpui::px(TILE_HEIGHT))
            .p_3()
            .gap_1()
            .rounded_md()
            .border_2()
            .border_color(border_color)
            .bg(bg)
            .child(
                Label::new(session.name.clone())
                    .size(LabelSize::Large)
                    .when(is_active, |label| label.color(Color::Accent)),
            )
            .child(
                Label::new(cwd_display)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Label::new(format!(
                            "{} window{}",
                            session.windows.len(),
                            if session.windows.len() == 1 { "" } else { "s" }
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    )
                    .child(
                        Label::new(last_attached)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
    }
}

impl EventEmitter<DismissEvent> for SessionOverviewModal {}
impl ModalView for SessionOverviewModal {}

impl Focusable for SessionOverviewModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SessionOverviewModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_id = SessionRegistry::global(cx).active_id();
        let theme = cx.theme();
        let viewport = window.viewport_size();
        let viewport_w = f32::from(viewport.width);
        let viewport_h = f32::from(viewport.height);
        // Snap the keyboard-nav column count to whatever fits the visible
        // grid so h/j/k/l matches what the user sees.
        self.columns = columns_for_viewport(viewport_w);
        let max_w = gpui::px((viewport_w * MODAL_W_FRAC).min(1200.0));
        let max_h = gpui::px(viewport_h * MODAL_H_FRAC);

        // Compose the grid as a row-major flex_wrap. We don't care that
        // flexbox doesn't strictly align rows — `columns` is only a hint
        // for keyboard navigation.
        let tiles: Vec<_> = self
            .sessions
            .iter()
            .enumerate()
            .map(|(ix, session)| self.render_tile(ix, session, active_id, cx).into_any_element())
            .collect();

        let header = h_flex()
            .w_full()
            .justify_between()
            .px_1()
            .pb_2()
            .child(Label::new("Sessions").size(LabelSize::Large))
            .child(
                Label::new("h/j/k/l move · Enter attach · Esc dismiss")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );

        let body = if self.sessions.is_empty() {
            div()
                .p_8()
                .child(
                    Label::new("No sessions yet — `cmd-k s n` to create one.")
                        .color(Color::Muted),
                )
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap_3()
                .children(tiles)
                .into_any_element()
        };

        v_flex()
            .key_context("CodonSessionOverview")
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

fn matches_dismiss(keystroke: &Keystroke) -> bool {
    keystroke.key == "escape"
}

/// Truncate a string in the middle with an ellipsis when it exceeds
/// `max_chars`. Useful for cwd paths where the leading prefix and the
/// trailing directory name both carry information.
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

/// Render a unix-epoch-millis timestamp as a short relative string
/// ("just now", "5m ago", "2h ago", "3d ago"). Falls back to an absolute
/// date when the gap exceeds a week so the user still gets a frame of
/// reference for long-dormant sessions.
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

// TODO(phase-5/session-overview): `/` filter sub-mode — narrow the grid
// in place by fuzzy-matching session names. Deferred to keep this change
// under the ~300 LOC budget; the spec explicitly allows a follow-up.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_middle_short_string_is_unchanged() {
        assert_eq!(truncate_middle("/tmp/x", 36), "/tmp/x");
    }

    #[test]
    fn truncate_middle_long_string_keeps_head_and_tail() {
        let path = "/Users/someone/Devel/personal/codon_v3/crates/codon-session";
        let out = truncate_middle(path, 20);
        assert!(out.contains('…'));
        assert!(out.chars().count() <= 20);
        assert!(out.starts_with('/'));
        assert!(out.ends_with("session"));
    }

    #[test]
    fn format_relative_handles_zero() {
        assert_eq!(format_relative_ms(0), "—");
    }

    #[test]
    fn format_relative_recent_is_just_now() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        assert_eq!(format_relative_ms(now), "just now");
    }
}
