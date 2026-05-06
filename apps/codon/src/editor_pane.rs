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
use helix_view::{current_ref, Editor};

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
            Err(_) => { editor.new_file(helix_view::editor::Action::VerticalSplit); }
        }

        let focus_handle = cx.focus_handle();
        Self { editor, focus_handle }
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

        let file_path = doc.path()
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

        let mut line_elements: Vec<AnyElement> = Vec::new();
        for line_idx in first_line..last_line {
            let line_text: String = text.line(line_idx).to_string();
            let trimmed = line_text.trim_end_matches('\n').trim_end_matches('\r');
            let is_cursor_line = line_idx == cursor_line;

            let line_num_color = if is_cursor_line { rgb(0xcdd6f4) } else { rgb(0x6c7086) };
            let line_num = div()
                .w(px(40.0)).flex_shrink_0()
                .text_color(line_num_color)
                .child(SharedString::from(format!("{:>4}", line_idx + 1)));

            let content = if is_cursor_line && !trimmed.is_empty() {
                let byte_offset = trimmed.char_indices()
                    .nth(cursor_col).map(|(i, _)| i).unwrap_or(trimmed.len());
                let cursor_end = trimmed[byte_offset..]
                    .chars().next().map(|c| byte_offset + c.len_utf8()).unwrap_or(trimmed.len());

                let mut highlights = Vec::new();
                if self.editor.mode != Mode::Insert && byte_offset < trimmed.len() {
                    highlights.push((byte_offset..cursor_end, HighlightStyle {
                        background_color: Some(rgb(0xcdd6f4).into()),
                        color: Some(rgb(0x1e1e2e).into()),
                        ..Default::default()
                    }));
                }
                let styled = StyledText::new(SharedString::from(trimmed.to_string()))
                    .with_default_highlights(&window.text_style(), highlights);
                div().child(styled).into_any_element()
            } else if is_cursor_line && trimmed.is_empty() && self.editor.mode != Mode::Insert {
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

        // Mode key context
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
            .w_full().px_4().py_1().bg(rgb(0x181825))
            .flex().flex_row().justify_between()
            .child(div().flex().flex_row().gap_4()
                .child(div().px_2().bg(mode_bg).text_color(rgb(0x1e1e2e)).font_weight(FontWeight::BOLD)
                    .child(SharedString::from(mode_str.to_string())))
                .child(div().text_color(rgb(0xa6adc8)).child(SharedString::from(file_path))))
            .child(div().text_color(rgb(0xa6adc8))
                .child(SharedString::from(format!("{}:{}", cursor_line + 1, cursor_col + 1))));

        div()
            .id("editor-pane")
            .track_focus(&self.focus_handle)
            .key_context(key_ctx)
            .size_full().flex().flex_col()
            .bg(rgb(0x1e1e2e)).text_color(rgb(0xcdd6f4))
            .text_size(px(14.0)).font_family("Lilex").whitespace_nowrap()
            // Register ALL action handlers
            // Motion
            .on_action(cx.listener(Self::move_char_left))
            .on_action(cx.listener(Self::move_char_right))
            .on_action(cx.listener(Self::move_line_up))
            .on_action(cx.listener(Self::move_line_down))
            .on_action(cx.listener(Self::move_visual_line_up))
            .on_action(cx.listener(Self::move_visual_line_down))
            .on_action(cx.listener(Self::move_next_word_start))
            .on_action(cx.listener(Self::move_prev_word_start))
            .on_action(cx.listener(Self::move_next_word_end))
            .on_action(cx.listener(Self::move_prev_word_end))
            .on_action(cx.listener(Self::move_next_long_word_start))
            .on_action(cx.listener(Self::move_prev_long_word_start))
            .on_action(cx.listener(Self::move_next_long_word_end))
            .on_action(cx.listener(Self::move_prev_long_word_end))
            // Extend
            .on_action(cx.listener(Self::extend_char_left))
            .on_action(cx.listener(Self::extend_char_right))
            .on_action(cx.listener(Self::extend_line_up))
            .on_action(cx.listener(Self::extend_line_down))
            .on_action(cx.listener(Self::extend_visual_line_up))
            .on_action(cx.listener(Self::extend_visual_line_down))
            .on_action(cx.listener(Self::extend_next_word_start))
            .on_action(cx.listener(Self::extend_prev_word_start))
            .on_action(cx.listener(Self::extend_next_word_end))
            .on_action(cx.listener(Self::extend_prev_word_end))
            .on_action(cx.listener(Self::extend_next_long_word_start))
            .on_action(cx.listener(Self::extend_prev_long_word_start))
            .on_action(cx.listener(Self::extend_next_long_word_end))
            .on_action(cx.listener(Self::extend_prev_long_word_end))
            // Goto
            .on_action(cx.listener(Self::goto_file_start))
            .on_action(cx.listener(Self::goto_file_end))
            .on_action(cx.listener(Self::goto_last_line))
            .on_action(cx.listener(Self::goto_line_start))
            .on_action(cx.listener(Self::goto_line_end))
            .on_action(cx.listener(Self::goto_first_nonwhitespace))
            .on_action(cx.listener(Self::goto_next_paragraph))
            .on_action(cx.listener(Self::goto_prev_paragraph))
            .on_action(cx.listener(Self::goto_window_top))
            .on_action(cx.listener(Self::goto_window_center))
            .on_action(cx.listener(Self::goto_window_bottom))
            // Extend-to
            .on_action(cx.listener(Self::extend_to_line_start))
            .on_action(cx.listener(Self::extend_to_line_end))
            .on_action(cx.listener(Self::extend_to_first_nonwhitespace))
            .on_action(cx.listener(Self::extend_to_file_start))
            .on_action(cx.listener(Self::extend_to_file_end))
            .on_action(cx.listener(Self::extend_to_last_line))
            // Mode switching
            .on_action(cx.listener(Self::insert_mode))
            .on_action(cx.listener(Self::append_mode))
            .on_action(cx.listener(Self::normal_mode))
            .on_action(cx.listener(Self::select_mode))
            .on_action(cx.listener(Self::exit_select_mode))
            .on_action(cx.listener(Self::insert_at_line_start))
            .on_action(cx.listener(Self::insert_at_line_end))
            .on_action(cx.listener(Self::command_mode))
            // Editing
            .on_action(cx.listener(Self::delete_selection))
            .on_action(cx.listener(Self::delete_selection_noyank))
            .on_action(cx.listener(Self::change_selection))
            .on_action(cx.listener(Self::change_selection_noyank))
            .on_action(cx.listener(Self::delete_char_backward))
            .on_action(cx.listener(Self::delete_char_forward))
            .on_action(cx.listener(Self::delete_word_backward))
            .on_action(cx.listener(Self::delete_word_forward))
            .on_action(cx.listener(Self::kill_to_line_start))
            .on_action(cx.listener(Self::kill_to_line_end))
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::insert_tab))
            // Case
            .on_action(cx.listener(Self::switch_case))
            .on_action(cx.listener(Self::switch_to_uppercase))
            .on_action(cx.listener(Self::switch_to_lowercase))
            // Line ops
            .on_action(cx.listener(Self::open_below))
            .on_action(cx.listener(Self::open_above))
            .on_action(cx.listener(Self::add_newline_below))
            .on_action(cx.listener(Self::add_newline_above))
            .on_action(cx.listener(Self::join_selections))
            .on_action(cx.listener(Self::join_selections_space))
            // Indent
            .on_action(cx.listener(Self::indent_cmd))
            .on_action(cx.listener(Self::unindent_cmd))
            // Comments
            .on_action(cx.listener(Self::toggle_comments))
            .on_action(cx.listener(Self::toggle_line_comments))
            .on_action(cx.listener(Self::toggle_block_comments))
            // Undo/Redo
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::earlier))
            .on_action(cx.listener(Self::later))
            // Clipboard
            .on_action(cx.listener(Self::yank))
            .on_action(cx.listener(Self::yank_to_clipboard))
            .on_action(cx.listener(Self::yank_main_selection_to_clipboard))
            .on_action(cx.listener(Self::paste_after))
            .on_action(cx.listener(Self::paste_before))
            .on_action(cx.listener(Self::paste_clipboard_after))
            .on_action(cx.listener(Self::paste_clipboard_before))
            .on_action(cx.listener(Self::replace_with_yanked))
            .on_action(cx.listener(Self::replace_selections_with_clipboard))
            // Selection
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::collapse_selection))
            .on_action(cx.listener(Self::flip_selections))
            .on_action(cx.listener(Self::ensure_selections_forward))
            .on_action(cx.listener(Self::keep_primary_selection))
            .on_action(cx.listener(Self::remove_primary_selection))
            .on_action(cx.listener(Self::extend_line))
            .on_action(cx.listener(Self::extend_line_below))
            .on_action(cx.listener(Self::extend_line_above))
            .on_action(cx.listener(Self::extend_to_line_bounds))
            .on_action(cx.listener(Self::shrink_to_line_bounds))
            .on_action(cx.listener(Self::select_current_line))
            .on_action(cx.listener(Self::split_selection_on_newline))
            .on_action(cx.listener(Self::merge_selections))
            .on_action(cx.listener(Self::merge_consecutive_selections))
            .on_action(cx.listener(Self::rotate_selections_forward))
            .on_action(cx.listener(Self::rotate_selections_backward))
            // Search (stubs)
            .on_action(cx.listener(Self::search))
            .on_action(cx.listener(Self::rsearch))
            .on_action(cx.listener(Self::search_next))
            .on_action(cx.listener(Self::search_prev))
            .on_action(cx.listener(Self::extend_search_next))
            .on_action(cx.listener(Self::extend_search_prev))
            .on_action(cx.listener(Self::search_selection))
            // Find char (stubs)
            .on_action(cx.listener(Self::find_next_char))
            .on_action(cx.listener(Self::find_till_char))
            .on_action(cx.listener(Self::find_prev_char))
            .on_action(cx.listener(Self::till_prev_char))
            .on_action(cx.listener(Self::extend_next_char))
            .on_action(cx.listener(Self::extend_till_char))
            .on_action(cx.listener(Self::extend_prev_char))
            .on_action(cx.listener(Self::extend_till_prev_char))
            .on_action(cx.listener(Self::repeat_last_motion))
            // Match/surround (stubs)
            .on_action(cx.listener(Self::match_brackets))
            .on_action(cx.listener(Self::surround_add))
            .on_action(cx.listener(Self::surround_replace))
            .on_action(cx.listener(Self::surround_delete))
            .on_action(cx.listener(Self::select_textobject_around))
            .on_action(cx.listener(Self::select_textobject_inner))
            // Syntax tree (stubs)
            .on_action(cx.listener(Self::expand_selection))
            .on_action(cx.listener(Self::shrink_selection))
            .on_action(cx.listener(Self::select_next_sibling))
            .on_action(cx.listener(Self::select_prev_sibling))
            // Scroll/view
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::half_page_up))
            .on_action(cx.listener(Self::half_page_down))
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::align_view_middle))
            .on_action(cx.listener(Self::align_view_top))
            .on_action(cx.listener(Self::align_view_center))
            .on_action(cx.listener(Self::align_view_bottom))
            // Jumps
            .on_action(cx.listener(Self::jump_forward))
            .on_action(cx.listener(Self::jump_backward))
            .on_action(cx.listener(Self::save_selection))
            // Increment/decrement
            .on_action(cx.listener(Self::increment))
            .on_action(cx.listener(Self::decrement))
            // Replace (stub)
            .on_action(cx.listener(Self::replace))
            // No-op
            .on_action(cx.listener(Self::no_op))
            // Insert mode character input
            .on_key_down(cx.listener(Self::handle_key_down))
            // Content
            .child(div().id("editor-content").flex_1().overflow_y_scroll().p_2().children(line_elements))
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
