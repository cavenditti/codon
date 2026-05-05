mod editor_pane;
mod terminal_pane;

use std::borrow::Cow;
use std::path::Path;

use gpui::*;
use gpui_platform::application;

use editor_pane::EditorPane;
use terminal_pane::TerminalPane;

struct CodonApp {
    terminal: Entity<TerminalPane>,
    editor: Entity<EditorPane>,
}

impl Render for CodonApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(0x1e1e2e))
            .font_family("Lilex")
            .text_size(px(14.0))
            .text_color(rgb(0xcdd6f4))
            .child(
                div()
                    .w(relative(0.5))
                    .h_full()
                    .child(self.terminal.clone()),
            )
            .child(
                div()
                    .w(px(1.0))
                    .h_full()
                    .bg(rgb(0x45475a)),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .child(self.editor.clone()),
            )
    }
}

fn main() {
    // helix_view::Editor needs tokio for its handlers (word_index, etc.)
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    application().run(|cx: &mut App| {
        // Load a font for monospace rendering
        let fonts = vec![
            Cow::Borrowed(
                include_bytes!("../../../vendor/zed/assets/fonts/lilex/Lilex-Regular.ttf")
                    .as_slice(),
            ),
            Cow::Borrowed(
                include_bytes!("../../../vendor/zed/assets/fonts/lilex/Lilex-Bold.ttf").as_slice(),
            ),
            Cow::Borrowed(
                include_bytes!("../../../vendor/zed/assets/fonts/lilex/Lilex-Italic.ttf")
                    .as_slice(),
            ),
        ];
        cx.text_system()
            .add_fonts(fonts)
            .expect("failed to load fonts");

        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Codon".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let terminal = cx.new(|cx| TerminalPane::new(window, cx));
                    let editor =
                        cx.new(|cx| EditorPane::new(Path::new("/etc/hosts"), window, cx));
                    CodonApp { terminal, editor }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
