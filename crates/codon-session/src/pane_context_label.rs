use file_manager::FileManager;
use gpui::{Context, Empty, IntoElement, Render, SharedString, Window};
use terminal_view::TerminalView;
use ui::{Button, Color, LabelSize, Tooltip, prelude::*};
use util::paths::PathStyle;
use workspace::{ItemHandle, StatusItemView};

pub struct PaneContextLabel {
    caption: Option<Caption>,
}

#[derive(Clone)]
struct Caption {
    text: SharedString,
    tooltip: SharedString,
}

impl PaneContextLabel {
    pub fn new() -> Self {
        Self { caption: None }
    }
}

impl Default for PaneContextLabel {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for PaneContextLabel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(caption) = self.caption.clone() else {
            return Empty.into_any_element();
        };

        Button::new("pane-context-label", caption.text)
            .label_size(LabelSize::Small)
            .color(Color::Muted)
            .tooltip(Tooltip::text(caption.tooltip))
            .into_any_element()
    }
}

impl StatusItemView for PaneContextLabel {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.caption = active_pane_item.and_then(|item| caption_for(item, cx));
        cx.notify();
    }
}

fn caption_for(item: &dyn ItemHandle, cx: &gpui::App) -> Option<Caption> {
    if let Some(terminal_view) = item.downcast::<TerminalView>() {
        let cwd = terminal_view
            .read(cx)
            .terminal()
            .read(cx)
            .working_directory()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~".to_string());
        let display: SharedString = format!("term: {}", cwd).into();
        return Some(Caption {
            text: display.clone(),
            tooltip: display,
        });
    }

    if let Some(file_manager) = item.downcast::<FileManager>() {
        let cwd = file_manager
            .read(cx)
            .current_directory()
            .display()
            .to_string();
        let display: SharedString = format!("fm: {}", cwd).into();
        return Some(Caption {
            text: display.clone(),
            tooltip: display,
        });
    }

    if let Some(project_path) = item.project_path(cx) {
        let path: SharedString = project_path
            .path
            .display(PathStyle::local())
            .into_owned()
            .into();
        let tooltip = item.tab_tooltip_text(cx).unwrap_or_else(|| path.clone());
        return Some(Caption {
            text: path,
            tooltip,
        });
    }

    let text = item.tab_content_text(0, cx);
    if text.is_empty() {
        return None;
    }
    let tooltip = item.tab_tooltip_text(cx).unwrap_or_else(|| text.clone());
    Some(Caption { text, tooltip })
}
