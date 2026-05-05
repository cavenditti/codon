use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;
use gpui::*;
use helix_core::syntax::{self, config::Configuration};
use helix_view::document::Mode;
use helix_view::editor::Config;
use helix_view::graphics::Rect;
use helix_view::handlers::completion::{CompletionEvent, CompletionHandler};
use helix_view::handlers::{word_index, Handlers};
use helix_view::Editor;

pub struct EditorPane {
    pub editor: Editor,
    pub focus_handle: FocusHandle,
}

impl EditorPane {
    pub fn new(path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = Arc::new(ArcSwap::from_pointee(Config::default()));
        let syn_loader = Arc::new(ArcSwap::from_pointee(
            syntax::Loader::new(Configuration {
                language: vec![],
                language_server: Default::default(),
            })
            .expect("syntax loader init"),
        ));
        let theme_loader = Arc::new(helix_view::theme::Loader::new(&[]));
        let handlers = dummy_handlers();
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };

        let mut editor = Editor::new(area, theme_loader, syn_loader, config, handlers);

        // Open the file — must succeed to create a view in the tree
        // Use VerticalSplit for the first open — Action::Load assumes a view already exists.
        match editor.open(path, helix_view::editor::Action::VerticalSplit) {
            Ok(_) => {}
            Err(_) => {
                editor.new_file(helix_view::editor::Action::VerticalSplit);
            }
        }

        let focus_handle = cx.focus_handle();

        Self {
            editor,
            focus_handle,
        }
    }
}

impl Render for EditorPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (view, doc) = helix_view::current_ref!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let primary = selection.primary();
        let cursor_pos = primary.cursor(text.slice(..));
        let cursor_line = text.char_to_line(cursor_pos);
        let cursor_col = cursor_pos - text.line_to_char(cursor_line);
        let total_lines = text.len_lines();

        let file_path = doc
            .path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[scratch]".to_string());

        let mode_str = match self.editor.mode {
            Mode::Normal => "NOR",
            Mode::Insert => "INS",
            Mode::Select => "SEL",
        };

        // Build visible lines
        let visible_lines = 50.min(total_lines); // TODO: calculate from viewport
        let first_line = cursor_line.saturating_sub(visible_lines / 2);
        let last_line = (first_line + visible_lines).min(total_lines);

        let mut line_elements: Vec<Div> = Vec::new();
        for line_idx in first_line..last_line {
            let line_start = text.line_to_char(line_idx);
            let line_text: String = text.line(line_idx).to_string();
            let trimmed = line_text.trim_end_matches('\n').trim_end_matches('\r');

            let is_cursor_line = line_idx == cursor_line;

            let mut line_div = div().flex().flex_row().gap_3();

            // Line number
            let line_num_color = if is_cursor_line {
                rgb(0xcdd6f4)
            } else {
                rgb(0x6c7086)
            };
            line_div = line_div.child(
                div()
                    .w(px(40.0))
                    .flex_shrink_0()
                    .text_color(line_num_color)
                    .child(SharedString::from(format!("{:>4}", line_idx + 1))),
            );

            // Line content with cursor
            if is_cursor_line && !trimmed.is_empty() {
                // Build text with cursor highlight
                let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();

                // Cursor highlight (block cursor in normal mode)
                let col_byte = if cursor_col < trimmed.len() {
                    cursor_col
                } else {
                    trimmed.len().saturating_sub(1)
                };

                // Find byte offset of cursor column
                let mut byte_offset = 0;
                for (i, ch) in trimmed.char_indices() {
                    if i == col_byte || byte_offset >= col_byte {
                        byte_offset = i;
                        break;
                    }
                    byte_offset = i + ch.len_utf8();
                }
                let cursor_end = trimmed[byte_offset..]
                    .chars()
                    .next()
                    .map(|c| byte_offset + c.len_utf8())
                    .unwrap_or(byte_offset + 1);

                if self.editor.mode == Mode::Normal || self.editor.mode == Mode::Select {
                    // Block cursor
                    highlights.push((
                        byte_offset..cursor_end.min(trimmed.len()),
                        HighlightStyle {
                            background_color: Some(rgb(0xcdd6f4).into()),
                            color: Some(rgb(0x1e1e2e).into()),
                            ..Default::default()
                        },
                    ));
                }

                let styled = StyledText::new(SharedString::from(trimmed.to_string()))
                    .with_default_highlights(&window.text_style(), highlights);
                line_div = line_div.child(styled);
            } else {
                line_div =
                    line_div.child(SharedString::from(if trimmed.is_empty() {
                        " ".to_string()
                    } else {
                        trimmed.to_string()
                    }));
            }

            line_elements.push(line_div);
        }

        // Status bar at bottom
        let status = div()
            .w_full()
            .px_4()
            .py_1()
            .bg(rgb(0x181825))
            .flex()
            .flex_row()
            .justify_between()
            .child(
                div().flex().flex_row().gap_4().child(
                    div()
                        .px_2()
                        .bg(match self.editor.mode {
                            Mode::Normal => rgb(0x89b4fa),
                            Mode::Insert => rgb(0xa6e3a1),
                            Mode::Select => rgb(0xf5c2e7),
                        })
                        .text_color(rgb(0x1e1e2e))
                        .font_weight(FontWeight::BOLD)
                        .child(SharedString::from(mode_str.to_string())),
                ).child(
                    div()
                        .text_color(rgb(0xa6adc8))
                        .child(SharedString::from(file_path)),
                ),
            )
            .child(
                div()
                    .text_color(rgb(0xa6adc8))
                    .child(SharedString::from(format!(
                        "{}:{}",
                        cursor_line + 1,
                        cursor_col + 1
                    ))),
            );

        div()
            .id("editor-pane")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .text_size(px(14.0))
            .font_family("Lilex")
            .whitespace_nowrap()
            .child(
                div()
                    .id("editor-content")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_2()
                    .children(line_elements),
            )
            .child(status)
    }
}

/// Create dummy Handlers with no-op senders for Phase 1.
/// The receivers are dropped immediately, so events are silently discarded.
fn dummy_handlers() -> Handlers {
    let (completion_tx, _) = tokio::sync::mpsc::channel::<CompletionEvent>(1);
    let (sig_tx, _) = tokio::sync::mpsc::channel(1);
    let (auto_save_tx, _) = tokio::sync::mpsc::channel(1);
    let (doc_colors_tx, _) = tokio::sync::mpsc::channel(1);

    let (doc_links_tx, _) = tokio::sync::mpsc::channel(1);

    Handlers {
        completions: CompletionHandler::new(completion_tx),
        signature_hints: sig_tx,
        auto_save: auto_save_tx,
        document_colors: doc_colors_tx,
        document_links: doc_links_tx,
        word_index: word_index::Handler::spawn(),
        pull_diagnostics: tokio::sync::mpsc::channel(1).0,
        pull_all_documents_diagnostics: tokio::sync::mpsc::channel(1).0,
    }
}
