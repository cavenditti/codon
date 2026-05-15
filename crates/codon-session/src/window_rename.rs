use editor::Editor;
use gpui::{
    AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div,
};
use ui::{ActiveTheme as _, Color, FluentBuilder, Label, LabelCommon, LabelSize, StyledExt, h_flex, v_flex};
use workspace::{ModalView, Workspace};

use crate::{actions::persist_async, registry::SessionRegistry};

pub struct WindowRenameModal {
    editor: Entity<Editor>,
    workspace: gpui::WeakEntity<Workspace>,
    placeholder: String,
}

impl EventEmitter<DismissEvent> for WindowRenameModal {}
impl ModalView for WindowRenameModal {}

impl Focusable for WindowRenameModal {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl WindowRenameModal {
    pub fn new(
        workspace: gpui::WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let registry = SessionRegistry::global(cx);
        let current_name = registry
            .active()
            .and_then(|s| s.windows.get(s.active_window).map(|w| w.name.clone()))
            .unwrap_or_default();
        let placeholder = if current_name.is_empty() {
            "Window name".to_string()
        } else {
            current_name.clone()
        };

        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(placeholder.as_str(), window, cx);
            if !current_name.is_empty() {
                editor.set_text(current_name.as_str(), window, cx);
                editor.select_all(&editor::actions::SelectAll, window, cx);
            }
            editor
        });

        Self {
            editor,
            workspace,
            placeholder,
        }
    }

    fn cancel(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let new_name = self
            .editor
            .update(cx, |editor, cx| editor.text(cx).trim().to_string());

        if new_name.is_empty() {
            cx.emit(DismissEvent);
            return;
        }

        let registry = SessionRegistry::global(cx);
        let Some(active_id) = registry.active_id() else {
            cx.emit(DismissEvent);
            return;
        };
        let Some(mut session) = registry.get(active_id) else {
            cx.emit(DismissEvent);
            return;
        };
        if let Some(active) = session.active_mut() {
            active.name = new_name;
        }
        if let Err(err) = registry.upsert(session) {
            log::warn!("could not save window rename: {err:?}");
        }
        persist_async(cx);

        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |_, cx| cx.notify());
        }
        cx.emit(DismissEvent);
        let _ = window;
    }
}

impl Render for WindowRenameModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .key_context("WindowRenameModal")
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .elevation_3(cx)
            .w_96()
            .overflow_hidden()
            .child(
                div()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.colors().border_variant)
                    .child(self.editor.clone()),
            )
            .child(
                h_flex()
                    .bg(theme.colors().editor_background)
                    .rounded_b_sm()
                    .w_full()
                    .p_2()
                    .gap_1()
                    .when(true, |this| {
                        this.child(
                            Label::new(format!("Rename window (was: {})", self.placeholder))
                                .color(Color::Muted)
                                .size(LabelSize::Small),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod compile_assertions {
    use crate::actions::WindowRename;

    // Compile-time assertion: `WindowRename` is the action type the
    // window-rename modal reacts to. Catches accidental removal of
    // the action type while the modal still expects it.
    #[allow(dead_code)]
    fn assert_window_rename_action(_: &WindowRename) {}
}
