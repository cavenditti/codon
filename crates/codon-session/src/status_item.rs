use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
};
use ui::{Color, Label, LabelCommon, LabelSize, h_flex};
use workspace::{ItemHandle, StatusItemView, Workspace};

use crate::registry::SessionRegistry;

pub struct SessionStatusItem;

impl SessionStatusItem {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SessionStatusItem {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for SessionStatusItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = SessionRegistry::global(cx)
            .active()
            .map(|s| format!("⌘ {}", s.name))
            .unwrap_or_else(|| "⌘ —".to_string());
        h_flex().gap_1().child(
            Label::new(label)
                .color(Color::Muted)
                .size(LabelSize::Small),
        )
    }
}

impl StatusItemView for SessionStatusItem {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

/// Mount the session indicator on the workspace's status bar. Call from a
/// workspace registration hook (e.g. on `Workspace::Init`).
pub fn register(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<SessionStatusItem> {
    let item = cx.new(|_| SessionStatusItem::new());
    let handle = item.clone();
    workspace.status_bar().update(cx, |status_bar, cx| {
        status_bar.add_left_item(item, window, cx);
    });
    handle
}
