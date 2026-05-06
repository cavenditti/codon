mod editor_actions;
mod editor_pane;
mod terminal_pane;

use std::borrow::Cow;
use std::path::Path;

use gpui::*;
use gpui_platform::application;

use editor_actions::*;
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
            .child(div().w(relative(0.5)).h_full().child(self.terminal.clone()))
            .child(div().w(px(1.0)).h_full().bg(rgb(0x45475a)))
            .child(div().flex_1().h_full().child(self.editor.clone()))
    }
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    application().run(|cx: &mut App| {
        let fonts = vec![
            Cow::Borrowed(include_bytes!("../../../vendor/zed/assets/fonts/lilex/Lilex-Regular.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../../../vendor/zed/assets/fonts/lilex/Lilex-Bold.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../../../vendor/zed/assets/fonts/lilex/Lilex-Italic.ttf").as_slice()),
        ];
        cx.text_system().add_fonts(fonts).expect("failed to load fonts");

        // --- Key bindings ---
        // Normal mode editor bindings
        let normal = Some("EditorPane && mode == normal");
        // Insert mode editor bindings
        let insert = Some("EditorPane && mode == insert");

        cx.bind_keys([
            // Normal mode — motion
            KeyBinding::new("h", MoveCharLeft, normal),
            KeyBinding::new("left", MoveCharLeft, normal),
            KeyBinding::new("l", MoveCharRight, normal),
            KeyBinding::new("right", MoveCharRight, normal),
            KeyBinding::new("k", MoveVisualLineUp, normal),
            KeyBinding::new("up", MoveVisualLineUp, normal),
            KeyBinding::new("j", MoveVisualLineDown, normal),
            KeyBinding::new("down", MoveVisualLineDown, normal),
            KeyBinding::new("w", MoveNextWordStart, normal),
            KeyBinding::new("b", MovePrevWordStart, normal),
            KeyBinding::new("e", MoveNextWordEnd, normal),
            KeyBinding::new("g g", GotoFileStart, normal),
            KeyBinding::new("shift-g", GotoLastLine, normal),
            KeyBinding::new("0", GotoLineStart, normal),
            KeyBinding::new("$", GotoLineEnd, normal),
            // Normal mode — mode switching
            KeyBinding::new("i", InsertMode, normal),
            KeyBinding::new("a", AppendMode, normal),
            KeyBinding::new("o", OpenBelow, normal),
            KeyBinding::new("shift-o", OpenAbove, normal),
            // Normal mode — editing
            KeyBinding::new("x", DeleteSelection, normal),
            KeyBinding::new("d", DeleteSelection, normal),
            KeyBinding::new("c", ChangeSelection, normal),
            KeyBinding::new("u", Undo, normal),
            KeyBinding::new("shift-u", Redo, normal),
            // Normal mode — scrolling
            KeyBinding::new("ctrl-u", HalfPageUp, normal),
            KeyBinding::new("ctrl-d", HalfPageDown, normal),
            KeyBinding::new("ctrl-b", PageUp, normal),
            KeyBinding::new("ctrl-f", PageDown, normal),
            // Insert mode
            KeyBinding::new("escape", NormalMode, insert),
            KeyBinding::new("backspace", DeleteCharBackward, insert),
            KeyBinding::new("delete", DeleteCharForward, insert),
            KeyBinding::new("enter", InsertNewline, insert),
            KeyBinding::new("left", MoveCharLeft, insert),
            KeyBinding::new("right", MoveCharRight, insert),
            KeyBinding::new("up", MoveVisualLineUp, insert),
            KeyBinding::new("down", MoveVisualLineDown, insert),
            // Global
            KeyBinding::new("alt-h", FocusLeft, None),
            KeyBinding::new("alt-l", FocusRight, None),
        ]);

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
                    let editor = cx.new(|cx| EditorPane::new(Path::new("/etc/hosts"), window, cx));
                    CodonApp { terminal, editor }
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
