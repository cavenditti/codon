use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;
use gpui::*;
use helix_core::graphemes;
use helix_core::movement::{self, Direction, Movement};
use helix_core::syntax::{self, config::Configuration};
use helix_core::{Range, Selection, Tendril, Transaction};
use helix_view::document::Mode;
use helix_view::editor::Config;
use helix_view::graphics::Rect;
use helix_view::handlers::completion::{CompletionEvent, CompletionHandler};
use helix_view::handlers::{word_index, Handlers};
use helix_view::{current, current_ref, doc_mut, view, Editor};

use crate::editor_actions::*;

pub struct EditorPane {
    pub editor: Editor,
    pub focus_handle: FocusHandle,
}

impl EditorPane {
    pub fn new(path: &Path, _window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let area = Rect { x: 0, y: 0, width: 80, height: 24 };

        let mut editor = Editor::new(area, theme_loader, syn_loader, config, handlers);

        match editor.open(path, helix_view::editor::Action::VerticalSplit) {
            Ok(_) => {}
            Err(_) => {
                editor.new_file(helix_view::editor::Action::VerticalSplit);
            }
        }

        let focus_handle = cx.focus_handle();
        Self { editor, focus_handle }
    }

    // --- Motion commands ---

    fn move_char_left(&mut self, _: &MoveCharLeft, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Direction::Backward, Movement::Move, Self::move_h);
        cx.notify();
    }
    fn move_char_right(&mut self, _: &MoveCharRight, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Direction::Forward, Movement::Move, Self::move_h);
        cx.notify();
    }
    fn move_visual_line_up(&mut self, _: &MoveVisualLineUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Direction::Backward, Movement::Move, Self::move_v);
        cx.notify();
    }
    fn move_visual_line_down(&mut self, _: &MoveVisualLineDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Direction::Forward, Movement::Move, Self::move_v);
        cx.notify();
    }
    fn move_next_word_start(&mut self, _: &MoveNextWordStart, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            movement::move_next_word_start(text, range, 1)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    fn move_prev_word_start(&mut self, _: &MovePrevWordStart, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            movement::move_prev_word_start(text, range, 1)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    fn move_next_word_end(&mut self, _: &MoveNextWordEnd, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            movement::move_next_word_end(text, range, 1)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    fn goto_file_start(&mut self, _: &GotoFileStart, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = Selection::point(0);
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    fn goto_last_line(&mut self, _: &GotoLastLine, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let last_line = text.len_lines().saturating_sub(1);
        let pos = text.line_to_char(last_line);
        doc.set_selection(view.id, Selection::point(pos));
        cx.notify();
    }
    fn goto_line_start(&mut self, _: &GotoLineStart, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            let line = text.char_to_line(range.cursor(text));
            let pos = text.line_to_char(line);
            range.put_cursor(text, pos, false)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    fn goto_line_end(&mut self, _: &GotoLineEnd, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            let line = text.char_to_line(range.cursor(text));
            let end = helix_core::line_ending::line_end_char_index(&text, line);
            range.put_cursor(text, end, false)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }

    // --- Mode switching ---

    fn insert_mode(&mut self, _: &InsertMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    fn append_mode(&mut self, _: &AppendMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.editor.mode = Mode::Insert;
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            let pos = graphemes::next_grapheme_boundary(text, range.cursor(text));
            Range::new(pos, pos)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    fn normal_mode(&mut self, _: &NormalMode, _w: &mut Window, cx: &mut Context<Self>) {
        if self.editor.mode == Mode::Insert {
            // Append changes to history when leaving insert mode
            let (view, doc) = current!(self.editor);
            doc.append_changes_to_history(view);
        }
        self.editor.mode = Mode::Normal;
        // Collapse selection to cursor position
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            let pos = range.cursor(text);
            Range::new(pos, pos)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }

    // --- Editing ---

    fn delete_selection(&mut self, _: &DeleteSelection, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);

        // If selection is empty (cursor), delete the character under cursor
        let transaction = Transaction::change_by_selection(text, selection, |range| {
            let from = range.from();
            let to = if range.is_empty() {
                graphemes::next_grapheme_boundary(text.slice(..), from)
            } else {
                range.to()
            };
            (from, to, None)
        });
        doc.apply(&transaction, view.id);
        cx.notify();
    }
    fn change_selection(&mut self, _: &ChangeSelection, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection(&DeleteSelection, w, cx);
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    fn delete_char_backward(&mut self, _: &DeleteCharBackward, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let transaction = Transaction::change_by_selection(text, selection, |range| {
            let pos = range.cursor(text.slice(..));
            let prev = graphemes::prev_grapheme_boundary(text.slice(..), pos);
            (prev, pos, None)
        });
        doc.apply(&transaction, view.id);
        cx.notify();
    }
    fn delete_char_forward(&mut self, _: &DeleteCharForward, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let transaction = Transaction::change_by_selection(text, selection, |range| {
            let pos = range.cursor(text.slice(..));
            let next = graphemes::next_grapheme_boundary(text.slice(..), pos);
            (pos, next, None)
        });
        doc.apply(&transaction, view.id);
        cx.notify();
    }
    fn insert_newline(&mut self, _: &InsertNewline, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let line_ending = doc.line_ending.as_str();
        let transaction = Transaction::insert(text, selection, Tendril::from(line_ending));
        doc.apply(&transaction, view.id);
        cx.notify();
    }
    fn undo(&mut self, _: &Undo, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.undo(view);
        cx.notify();
    }
    fn redo(&mut self, _: &Redo, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.redo(view);
        cx.notify();
    }

    // --- Line operations ---

    fn open_below(&mut self, _: &OpenBelow, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let line_ending = doc.line_ending.as_str();
        // Move to end of current line, insert newline
        let transaction = Transaction::change_by_selection(text, selection, |range| {
            let line = text.char_to_line(range.cursor(text.slice(..)));
            let end = helix_core::line_ending::line_end_char_index(&text.slice(..), line);
            (end, end, Some(Tendril::from(line_ending)))
        });
        doc.apply(&transaction, view.id);
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    fn open_above(&mut self, _: &OpenAbove, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let line_ending = doc.line_ending.as_str();
        let transaction = Transaction::change_by_selection(text, selection, |range| {
            let line = text.char_to_line(range.cursor(text.slice(..)));
            let start = text.line_to_char(line);
            (start, start, Some(Tendril::from(line_ending)))
        });
        doc.apply(&transaction, view.id);
        // Move cursor up to the newly created line
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            let pos = range.cursor(text);
            let prev_line = text.char_to_line(pos).saturating_sub(1);
            range.put_cursor(text, text.line_to_char(prev_line), false)
        });
        doc.set_selection(view.id, selection);
        self.editor.mode = Mode::Insert;
        cx.notify();
    }

    // --- Scrolling ---

    fn page_up(&mut self, _: &PageUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(-24);
        cx.notify();
    }
    fn page_down(&mut self, _: &PageDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(24);
        cx.notify();
    }
    fn half_page_up(&mut self, _: &HalfPageUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(-12);
        cx.notify();
    }
    fn half_page_down(&mut self, _: &HalfPageDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(12);
        cx.notify();
    }

    // --- Insert mode character input ---

    fn handle_key_down(&mut self, event: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.editor.mode != Mode::Insert {
            return;
        }
        if let Some(key_char) = &event.keystroke.key_char {
            if event.keystroke.modifiers.control || event.keystroke.modifiers.platform {
                return;
            }
            for ch in key_char.chars() {
                let (view, doc) = current!(self.editor);
                let text = doc.text();
                let selection = doc.selection(view.id);
                let transaction = Transaction::insert(text, selection, Tendril::from_iter([ch]));
                doc.apply(&transaction, view.id);
            }
            cx.notify();
        }
    }

    // --- Helpers ---

    fn move_h(text: helix_core::ropey::RopeSlice, range: Range, dir: Direction, movement: Movement) -> Range {
        let text_fmt = helix_core::doc_formatter::TextFormat::default();
        let mut annotations = helix_core::text_annotations::TextAnnotations::default();
        movement::move_horizontally(text, range, dir, 1, movement, &text_fmt, &mut annotations)
    }

    fn move_v(text: helix_core::ropey::RopeSlice, range: Range, dir: Direction, movement: Movement) -> Range {
        let text_fmt = helix_core::doc_formatter::TextFormat::default();
        let mut annotations = helix_core::text_annotations::TextAnnotations::default();
        movement::move_vertically(text, range, dir, 1, movement, &text_fmt, &mut annotations)
    }

    fn move_selection(
        &mut self,
        dir: Direction,
        movement: Movement,
        mover: fn(helix_core::ropey::RopeSlice, Range, Direction, Movement) -> Range,
    ) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            mover(text, range, dir, movement)
        });
        doc.set_selection(view.id, selection);
    }

    fn scroll_lines(&mut self, delta: i32) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let cursor = doc.selection(view.id).primary().cursor(text);
        let line = text.char_to_line(cursor);
        let new_line = if delta > 0 {
            (line + delta as usize).min(text.len_lines().saturating_sub(1))
        } else {
            line.saturating_sub((-delta) as usize)
        };
        let new_pos = text.line_to_char(new_line);
        doc.set_selection(view.id, Selection::point(new_pos));
    }
}

impl Render for EditorPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (view, doc) = current_ref!(self.editor);
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

        // Center viewport around cursor
        let visible_lines = 40.min(total_lines);
        let first_line = cursor_line.saturating_sub(visible_lines / 2);
        let last_line = (first_line + visible_lines).min(total_lines);

        // Build line elements
        let mut line_elements: Vec<AnyElement> = Vec::new();
        for line_idx in first_line..last_line {
            let line_text: String = text.line(line_idx).to_string();
            let trimmed = line_text.trim_end_matches('\n').trim_end_matches('\r');
            let is_cursor_line = line_idx == cursor_line;

            let line_num_color = if is_cursor_line { rgb(0xcdd6f4) } else { rgb(0x6c7086) };

            let line_num = div()
                .w(px(40.0))
                .flex_shrink_0()
                .text_color(line_num_color)
                .child(SharedString::from(format!("{:>4}", line_idx + 1)));

            let content = if is_cursor_line && !trimmed.is_empty() {
                // Find byte offset for cursor column
                let byte_offset = trimmed.char_indices()
                    .nth(cursor_col)
                    .map(|(i, _)| i)
                    .unwrap_or(trimmed.len());
                let cursor_end = trimmed[byte_offset..]
                    .chars()
                    .next()
                    .map(|c| byte_offset + c.len_utf8())
                    .unwrap_or(trimmed.len());

                let mut highlights = Vec::new();
                if self.editor.mode != Mode::Insert && byte_offset < trimmed.len() {
                    highlights.push((
                        byte_offset..cursor_end,
                        HighlightStyle {
                            background_color: Some(rgb(0xcdd6f4).into()),
                            color: Some(rgb(0x1e1e2e).into()),
                            ..Default::default()
                        },
                    ));
                }

                let styled = StyledText::new(SharedString::from(trimmed.to_string()))
                    .with_default_highlights(&window.text_style(), highlights);
                div().child(styled).into_any_element()
            } else if is_cursor_line && trimmed.is_empty() && self.editor.mode != Mode::Insert {
                // Cursor on empty line — show block cursor
                let styled = StyledText::new(SharedString::from(" "))
                    .with_default_highlights(&window.text_style(), vec![
                        (0..1, HighlightStyle {
                            background_color: Some(rgb(0xcdd6f4).into()),
                            ..Default::default()
                        }),
                    ]);
                div().child(styled).into_any_element()
            } else {
                div().child(SharedString::from(if trimmed.is_empty() { " " } else { trimmed }.to_string())).into_any_element()
            };

            line_elements.push(
                div().flex().flex_row().gap_3().child(line_num).child(content).into_any_element()
            );
        }

        // Mode and key context
        let mut key_ctx = KeyContext::default();
        key_ctx.add("EditorPane");
        key_ctx.set("mode", match self.editor.mode {
            Mode::Normal => "normal",
            Mode::Insert => "insert",
            Mode::Select => "select",
        });

        // Status bar
        let mode_bg = match self.editor.mode {
            Mode::Normal => rgb(0x89b4fa),
            Mode::Insert => rgb(0xa6e3a1),
            Mode::Select => rgb(0xf5c2e7),
        };
        let status = div()
            .w_full().px_4().py_1()
            .bg(rgb(0x181825))
            .flex().flex_row().justify_between()
            .child(
                div().flex().flex_row().gap_4()
                    .child(div().px_2().bg(mode_bg).text_color(rgb(0x1e1e2e)).font_weight(FontWeight::BOLD)
                        .child(SharedString::from(mode_str.to_string())))
                    .child(div().text_color(rgb(0xa6adc8))
                        .child(SharedString::from(file_path)))
            )
            .child(div().text_color(rgb(0xa6adc8))
                .child(SharedString::from(format!("{}:{}", cursor_line + 1, cursor_col + 1))));

        div()
            .id("editor-pane")
            .track_focus(&self.focus_handle)
            .key_context(key_ctx)
            .size_full()
            .flex().flex_col()
            .bg(rgb(0x1e1e2e))
            .text_color(rgb(0xcdd6f4))
            .text_size(px(14.0))
            .font_family("Lilex")
            .whitespace_nowrap()
            // Register all action handlers
            .on_action(cx.listener(Self::move_char_left))
            .on_action(cx.listener(Self::move_char_right))
            .on_action(cx.listener(Self::move_visual_line_up))
            .on_action(cx.listener(Self::move_visual_line_down))
            .on_action(cx.listener(Self::move_next_word_start))
            .on_action(cx.listener(Self::move_prev_word_start))
            .on_action(cx.listener(Self::move_next_word_end))
            .on_action(cx.listener(Self::goto_file_start))
            .on_action(cx.listener(Self::goto_last_line))
            .on_action(cx.listener(Self::goto_line_start))
            .on_action(cx.listener(Self::goto_line_end))
            .on_action(cx.listener(Self::insert_mode))
            .on_action(cx.listener(Self::append_mode))
            .on_action(cx.listener(Self::normal_mode))
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::change_selection))
            .on_action(cx.listener(Self::delete_char_backward))
            .on_action(cx.listener(Self::delete_char_forward))
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::open_below))
            .on_action(cx.listener(Self::open_above))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::half_page_up))
            .on_action(cx.listener(Self::half_page_down))
            .on_key_down(cx.listener(Self::handle_key_down))
            // Content
            .child(
                div().id("editor-content").flex_1().overflow_y_scroll().p_2()
                    .children(line_elements)
            )
            .child(status)
    }
}

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
