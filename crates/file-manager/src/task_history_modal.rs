//! `w` modal listing the file-manager's recent fs tasks.
//!
//! The modal is a snapshot of `FmTaskStore` taken at open time —
//! ticking progress on a still-running task does not redraw the modal,
//! which keeps the list stable while the user is reading it. Pressing
//! Enter on a finished row re-emits that task's resolution
//! notification through the same helper the live loop uses, so the
//! frame looks identical to the one the user dismissed.

use gpui::{
    AnyElement, App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyContext, KeyDownEvent, ParentElement, Render, SharedString,
    Styled, WeakEntity, Window, div, prelude::FluentBuilder, px,
};
use std::time::Instant;
use ui::{ActiveTheme, Color, Label, LabelCommon, LabelSize, h_flex, v_flex};
use workspace::{ModalView, Workspace};

use crate::tasks::{FmTask, FmTaskState, FmTaskStore, emit_resolution};

const ROW_HEIGHT_PX: f32 = 26.0;

pub struct TaskHistoryModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    rows: Vec<FmTask>,
    cursor: usize,
}

impl TaskHistoryModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let rows = cx.global::<FmTaskStore>().snapshot();
        Self {
            focus_handle: cx.focus_handle(),
            workspace,
            rows,
            cursor: 0,
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn move_cursor(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let len = self.rows.len() as isize;
        let next = (self.cursor as isize + delta).clamp(0, len - 1);
        self.cursor = next as usize;
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(task) = self.rows.get(self.cursor).cloned() else {
            return;
        };
        // Running tasks already have a live notification — re-emitting
        // a "Running 4 of 12 …" frame on Enter is more noise than
        // value, so confirm is a no-op for them. Terminal frames are
        // the interesting case: the user dismissed the original
        // resolution toast and wants it back.
        if task.is_terminal() {
            let workspace = self.workspace.clone();
            cx.defer(move |cx| {
                emit_resolution(task, workspace, cx);
            });
        }
        self.dismiss(cx);
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => self.dismiss(cx),
            "j" | "down" => self.move_cursor(1, cx),
            "k" | "up" => self.move_cursor(-1, cx),
            "g" => {
                self.cursor = 0;
                cx.notify();
            }
            "G" => {
                self.cursor = self.rows.len().saturating_sub(1);
                cx.notify();
            }
            "enter" | "\n" => self.confirm(cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn dispatch_context(&self) -> KeyContext {
        let mut ctx = KeyContext::new_with_defaults();
        ctx.add("FileManagerTaskHistory");
        ctx
    }
}

impl ModalView for TaskHistoryModal {}
impl EventEmitter<DismissEvent> for TaskHistoryModal {}

impl Focusable for TaskHistoryModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TaskHistoryModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let rows: Vec<AnyElement> = if self.rows.is_empty() {
            vec![
                Label::new(SharedString::from("No file-manager tasks yet."))
                    .color(Color::Muted)
                    .into_any_element(),
            ]
        } else {
            self.rows
                .iter()
                .enumerate()
                .map(|(i, task)| render_row(task, i == self.cursor, &theme).into_any_element())
                .collect()
        };

        v_flex()
            .key_context(self.dispatch_context())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .w(px(640.))
            .max_h(px(480.))
            .bg(theme.colors().elevated_surface_background)
            .border_1()
            .border_color(theme.colors().border)
            .rounded_md()
            .p_3()
            .gap_1()
            .child(
                Label::new(SharedString::from("File-manager tasks"))
                    .size(LabelSize::Large)
                    .color(Color::Default),
            )
            .child(
                Label::new(SharedString::from(
                    "Last 50 fs tasks · j/k to move · Enter to re-emit · Esc to close",
                ))
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .children(rows)
    }
}

fn render_row(task: &FmTask, is_cursor: bool, theme: &theme::Theme) -> impl IntoElement {
    let status_color = match task.state {
        FmTaskState::Running { .. } => Color::Info,
        FmTaskState::Done { .. } => Color::Success,
        FmTaskState::Failed { .. } => Color::Error,
        FmTaskState::Cancelled { .. } => Color::Warning,
    };
    let bg = if is_cursor {
        Some(theme.colors().ghost_element_selected)
    } else {
        None
    };
    let duration_label = format_duration(task);
    h_flex()
        .h(px(ROW_HEIGHT_PX))
        .px_2()
        .gap_3()
        .when_some(bg, |this, bg| this.bg(bg))
        .child(
            div()
                .min_w(px(280.))
                .flex_none()
                .child(Label::new(SharedString::from(task.summary())).size(LabelSize::Default)),
        )
        .child(
            div().min_w(px(80.)).flex_none().child(
                Label::new(SharedString::from(task.state.status_label()))
                    .color(status_color)
                    .size(LabelSize::Small),
            ),
        )
        .child(
            Label::new(SharedString::from(duration_label))
                .color(Color::Muted)
                .size(LabelSize::Small),
        )
}

fn format_duration(task: &FmTask) -> String {
    let end = task.completed_at.unwrap_or_else(Instant::now);
    let secs = end.saturating_duration_since(task.started_at).as_secs();
    let mm = secs / 60;
    let ss = secs % 60;
    format!("{mm:02}:{ss:02}")
}
