//! Editor command implementations using helix-core/helix-view APIs.
//! Each command is a method on EditorPane following the GPUI action handler signature.

use gpui::*;
use helix_core::graphemes;
use helix_core::movement::{self, Direction, Movement};
use helix_core::{Range, Selection, Tendril, Transaction};
use helix_stdx::rope::RopeSliceExt;
use helix_view::document::Mode;
use helix_view::{current, current_ref};

use crate::editor_actions::*;
use crate::editor_pane::EditorPane;

// ── Helpers ──────────────────────────────────────────────────────────────────

type RopeSlice<'a> = helix_core::ropey::RopeSlice<'a>;

fn text_fmt() -> helix_core::doc_formatter::TextFormat {
    helix_core::doc_formatter::TextFormat::default()
}

fn text_ann<'a>() -> helix_core::text_annotations::TextAnnotations<'a> {
    helix_core::text_annotations::TextAnnotations::default()
}

/// Apply a motion that transforms each range in the selection.
fn apply_motion(
    editor: &mut helix_view::Editor,
    f: impl Fn(RopeSlice, Range) -> Range,
) {
    let (view, doc) = current!(editor);
    let text = doc.text().slice(..);
    let selection = doc.selection(view.id).clone().transform(|range| f(text, range));
    doc.set_selection(view.id, selection);
}

/// Apply a transaction produced from the current selection.
fn apply_change(
    editor: &mut helix_view::Editor,
    f: impl Fn(&helix_core::Rope, &Selection) -> Transaction,
) {
    let (view, doc) = current!(editor);
    let text = doc.text();
    let selection = doc.selection(view.id);
    let transaction = f(text, selection);
    doc.apply(&transaction, view.id);
}

// ── Motion commands ──────────────────────────────────────────────────────────

macro_rules! motion_handler {
    ($name:ident, $action:ty, $body:expr) => {
        pub fn $name(&mut self, _: &$action, _w: &mut Window, cx: &mut Context<Self>) {
            apply_motion(&mut self.editor, $body);
            cx.notify();
        }
    };
}

macro_rules! horizontal_motion {
    ($name:ident, $action:ty, $dir:expr, $movement:expr) => {
        motion_handler!($name, $action, |text, range| {
            movement::move_horizontally(text, range, $dir, 1, $movement, &text_fmt(), &mut text_ann())
        });
    };
}

macro_rules! vertical_motion {
    ($name:ident, $action:ty, $dir:expr, $movement:expr) => {
        motion_handler!($name, $action, |text, range| {
            movement::move_vertically(text, range, $dir, 1, $movement, &text_fmt(), &mut text_ann())
        });
    };
}

macro_rules! word_motion {
    ($name:ident, $action:ty, $func:path) => {
        motion_handler!($name, $action, |text, range| {
            $func(text, range, 1)
        });
    };
}

impl EditorPane {
    // Horizontal
    horizontal_motion!(move_char_left,  MoveCharLeft,  Direction::Backward, Movement::Move);
    horizontal_motion!(move_char_right, MoveCharRight, Direction::Forward,  Movement::Move);
    horizontal_motion!(extend_char_left,  ExtendCharLeft,  Direction::Backward, Movement::Extend);
    horizontal_motion!(extend_char_right, ExtendCharRight, Direction::Forward,  Movement::Extend);

    // Vertical
    vertical_motion!(move_line_up,    MoveLineUp,    Direction::Backward, Movement::Move);
    vertical_motion!(move_line_down,  MoveLineDown,  Direction::Forward,  Movement::Move);
    vertical_motion!(move_visual_line_up,   MoveVisualLineUp,   Direction::Backward, Movement::Move);
    vertical_motion!(move_visual_line_down, MoveVisualLineDown, Direction::Forward,  Movement::Move);
    vertical_motion!(extend_line_up,   ExtendLineUp,   Direction::Backward, Movement::Extend);
    vertical_motion!(extend_line_down, ExtendLineDown, Direction::Forward,  Movement::Extend);
    vertical_motion!(extend_visual_line_up,   ExtendVisualLineUp,   Direction::Backward, Movement::Extend);
    vertical_motion!(extend_visual_line_down, ExtendVisualLineDown, Direction::Forward,  Movement::Extend);

    // Word motions
    word_motion!(move_next_word_start,  MoveNextWordStart,  movement::move_next_word_start);
    word_motion!(move_prev_word_start,  MovePrevWordStart,  movement::move_prev_word_start);
    word_motion!(move_next_word_end,    MoveNextWordEnd,    movement::move_next_word_end);
    word_motion!(move_prev_word_end,    MovePrevWordEnd,    movement::move_prev_word_end);
    word_motion!(move_next_long_word_start, MoveNextLongWordStart, movement::move_next_long_word_start);
    word_motion!(move_prev_long_word_start, MovePrevLongWordStart, movement::move_prev_long_word_start);
    word_motion!(move_next_long_word_end,   MoveNextLongWordEnd,   movement::move_next_long_word_end);
    word_motion!(move_prev_long_word_end,   MovePrevLongWordEnd,   movement::move_prev_long_word_end);
    // Extend word
    word_motion!(extend_next_word_start,  ExtendNextWordStart,  movement::move_next_word_start);
    word_motion!(extend_prev_word_start,  ExtendPrevWordStart,  movement::move_prev_word_start);
    word_motion!(extend_next_word_end,    ExtendNextWordEnd,    movement::move_next_word_end);
    word_motion!(extend_prev_word_end,    ExtendPrevWordEnd,    movement::move_prev_word_end);
    word_motion!(extend_next_long_word_start, ExtendNextLongWordStart, movement::move_next_long_word_start);
    word_motion!(extend_prev_long_word_start, ExtendPrevLongWordStart, movement::move_prev_long_word_start);
    word_motion!(extend_next_long_word_end,   ExtendNextLongWordEnd,   movement::move_next_long_word_end);
    word_motion!(extend_prev_long_word_end,   ExtendPrevLongWordEnd,   movement::move_prev_long_word_end);

    // Paragraph
    motion_handler!(goto_next_paragraph, GotoNextParagraph, |text, range| {
        movement::move_next_paragraph(text, range, 1, Movement::Move)
    });
    motion_handler!(goto_prev_paragraph, GotoPrevParagraph, |text, range| {
        movement::move_prev_paragraph(text, range, 1, Movement::Move)
    });

    // ── Goto commands ────────────────────────────────────────────────────────

    pub fn goto_file_start(&mut self, _: &GotoFileStart, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.set_selection(view.id, Selection::point(0));
        cx.notify();
    }
    pub fn goto_file_end(&mut self, _: &GotoFileEnd, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let end = doc.text().len_chars().saturating_sub(1);
        doc.set_selection(view.id, Selection::point(end));
        cx.notify();
    }
    pub fn goto_last_line(&mut self, _: &GotoLastLine, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let last_line = doc.text().len_lines().saturating_sub(1);
        let pos = doc.text().line_to_char(last_line);
        doc.set_selection(view.id, Selection::point(pos));
        cx.notify();
    }
    pub fn goto_line_start(&mut self, _: &GotoLineStart, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            let pos = text.line_to_char(line);
            range.put_cursor(text, pos, false)
        });
        cx.notify();
    }
    pub fn goto_line_end(&mut self, _: &GotoLineEnd, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            let end = helix_core::line_ending::line_end_char_index(&text, line);
            range.put_cursor(text, end, false)
        });
        cx.notify();
    }
    pub fn goto_first_nonwhitespace(&mut self, _: &GotoFirstNonwhitespace, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            let start = text.line_to_char(line);
            let pos = text.line(line).first_non_whitespace_char()
                .map(|offset| start + offset)
                .unwrap_or(start);
            range.put_cursor(text, pos, false)
        });
        cx.notify();
    }
    // Extend-to variants
    pub fn extend_to_line_start(&mut self, _: &ExtendToLineStart, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            range.put_cursor(text, text.line_to_char(line), true)
        });
        cx.notify();
    }
    pub fn extend_to_line_end(&mut self, _: &ExtendToLineEnd, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            let end = helix_core::line_ending::line_end_char_index(&text, line);
            range.put_cursor(text, end, true)
        });
        cx.notify();
    }
    pub fn extend_to_first_nonwhitespace(&mut self, _: &ExtendToFirstNonwhitespace, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            let start = text.line_to_char(line);
            let pos = text.line(line).first_non_whitespace_char()
                .map(|offset| start + offset)
                .unwrap_or(start);
            range.put_cursor(text, pos, true)
        });
        cx.notify();
    }
    pub fn extend_to_file_start(&mut self, _: &ExtendToFileStart, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| range.put_cursor(text, 0, true));
        cx.notify();
    }
    pub fn extend_to_file_end(&mut self, _: &ExtendToFileEnd, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            range.put_cursor(text, text.len_chars().saturating_sub(1), true)
        });
        cx.notify();
    }
    pub fn extend_to_last_line(&mut self, _: &ExtendToLastLine, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let last = text.len_lines().saturating_sub(1);
            range.put_cursor(text, text.line_to_char(last), true)
        });
        cx.notify();
    }

    // ── Mode switching ───────────────────────────────────────────────────────

    pub fn insert_mode(&mut self, _: &InsertMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    pub fn append_mode(&mut self, _: &AppendMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.editor.mode = Mode::Insert;
        apply_motion(&mut self.editor, |text, range| {
            let pos = graphemes::next_grapheme_boundary(text, range.cursor(text));
            Range::new(pos, pos)
        });
        cx.notify();
    }
    pub fn normal_mode(&mut self, _: &NormalMode, _w: &mut Window, cx: &mut Context<Self>) {
        if self.editor.mode == Mode::Insert {
            let (view, doc) = current!(self.editor);
            doc.append_changes_to_history(view);
        }
        self.editor.mode = Mode::Normal;
        apply_motion(&mut self.editor, |text, range| {
            let pos = range.cursor(text);
            Range::new(pos, pos)
        });
        cx.notify();
    }
    pub fn select_mode(&mut self, _: &SelectMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.editor.mode = Mode::Select;
        cx.notify();
    }
    pub fn exit_select_mode(&mut self, _: &ExitSelectMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.editor.mode = Mode::Normal;
        cx.notify();
    }
    pub fn insert_at_line_start(&mut self, _: &InsertAtLineStart, _w: &mut Window, cx: &mut Context<Self>) {
        self.goto_first_nonwhitespace(&GotoFirstNonwhitespace, _w, cx);
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    pub fn insert_at_line_end(&mut self, _: &InsertAtLineEnd, _w: &mut Window, cx: &mut Context<Self>) {
        self.goto_line_end(&GotoLineEnd, _w, cx);
        self.editor.mode = Mode::Insert;
        apply_motion(&mut self.editor, |text, range| {
            let pos = graphemes::next_grapheme_boundary(text, range.cursor(text));
            Range::new(pos, pos)
        });
        cx.notify();
    }

    // ── Editing commands ─────────────────────────────────────────────────────

    pub fn delete_selection(&mut self, _: &DeleteSelection, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let from = range.from();
                let to = if range.is_empty() {
                    graphemes::next_grapheme_boundary(text.slice(..), from)
                } else {
                    range.to()
                };
                (from, to, None)
            })
        });
        cx.notify();
    }
    pub fn delete_selection_noyank(&mut self, _: &DeleteSelectionNoyank, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection(&DeleteSelection, w, cx);
    }
    pub fn change_selection(&mut self, _: &ChangeSelection, w: &mut Window, cx: &mut Context<Self>) {
        self.delete_selection(&DeleteSelection, w, cx);
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    pub fn change_selection_noyank(&mut self, _: &ChangeSelectionNoyank, w: &mut Window, cx: &mut Context<Self>) {
        self.change_selection(&ChangeSelection, w, cx);
    }
    pub fn delete_char_backward(&mut self, _: &DeleteCharBackward, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                let prev = graphemes::prev_grapheme_boundary(text.slice(..), pos);
                (prev, pos, None)
            })
        });
        cx.notify();
    }
    pub fn delete_char_forward(&mut self, _: &DeleteCharForward, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                let next = graphemes::next_grapheme_boundary(text.slice(..), pos);
                (pos, next, None)
            })
        });
        cx.notify();
    }
    pub fn delete_word_backward(&mut self, _: &DeleteWordBackward, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                let prev = movement::move_prev_word_start(text.slice(..), *range, 1);
                (prev.cursor(text.slice(..)), pos, None)
            })
        });
        cx.notify();
    }
    pub fn delete_word_forward(&mut self, _: &DeleteWordForward, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                let next = movement::move_next_word_start(text.slice(..), *range, 1);
                (pos, next.cursor(text.slice(..)), None)
            })
        });
        cx.notify();
    }
    pub fn kill_to_line_start(&mut self, _: &KillToLineStart, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                let line = text.char_to_line(pos);
                let start = text.line_to_char(line);
                (start, pos, None)
            })
        });
        cx.notify();
    }
    pub fn kill_to_line_end(&mut self, _: &KillToLineEnd, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                let line = text.char_to_line(pos);
                let end = helix_core::line_ending::line_end_char_index(&text.slice(..), line);
                (pos, end, None)
            })
        });
        cx.notify();
    }
    pub fn insert_newline(&mut self, _: &InsertNewline, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let le = doc.line_ending.as_str();
        let text = doc.text();
        let selection = doc.selection(view.id);
        let transaction = Transaction::insert(text, selection, Tendril::from(le));
        doc.apply(&transaction, view.id);
        cx.notify();
    }
    pub fn insert_tab(&mut self, _: &InsertTab, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let transaction = Transaction::insert(text, selection, Tendril::from("\t"));
        doc.apply(&transaction, view.id);
        cx.notify();
    }

    // ── Case conversion ──────────────────────────────────────────────────────

    pub fn switch_case(&mut self, _: &SwitchCase, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let text_slice = text.slice(range.from()..range.to());
                let swapped: String = text_slice.chars().map(|c| {
                    if c.is_lowercase() { c.to_uppercase().next().unwrap_or(c) }
                    else { c.to_lowercase().next().unwrap_or(c) }
                }).collect();
                (range.from(), range.to(), Some(Tendril::from(swapped)))
            })
        });
        cx.notify();
    }
    pub fn switch_to_uppercase(&mut self, _: &SwitchToUppercase, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let s: String = text.slice(range.from()..range.to()).chars().flat_map(|c| c.to_uppercase()).collect();
                (range.from(), range.to(), Some(Tendril::from(s)))
            })
        });
        cx.notify();
    }
    pub fn switch_to_lowercase(&mut self, _: &SwitchToLowercase, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let s: String = text.slice(range.from()..range.to()).chars().flat_map(|c| c.to_lowercase()).collect();
                (range.from(), range.to(), Some(Tendril::from(s)))
            })
        });
        cx.notify();
    }

    // ── Line operations ──────────────────────────────────────────────────────

    pub fn open_below(&mut self, _: &OpenBelow, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            let le = "\n"; // simplified
            Transaction::change_by_selection(text, selection, |range| {
                let line = text.char_to_line(range.cursor(text.slice(..)));
                let end = helix_core::line_ending::line_end_char_index(&text.slice(..), line);
                (end, end, Some(Tendril::from(le)))
            })
        });
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    pub fn open_above(&mut self, _: &OpenAbove, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            let le = "\n";
            Transaction::change_by_selection(text, selection, |range| {
                let line = text.char_to_line(range.cursor(text.slice(..)));
                let start = text.line_to_char(line);
                (start, start, Some(Tendril::from(le)))
            })
        });
        self.editor.mode = Mode::Insert;
        cx.notify();
    }
    pub fn add_newline_below(&mut self, _: &AddNewlineBelow, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let line = text.char_to_line(range.cursor(text.slice(..)));
                let end = helix_core::line_ending::line_end_char_index(&text.slice(..), line);
                (end, end, Some(Tendril::from("\n")))
            })
        });
        cx.notify();
    }
    pub fn add_newline_above(&mut self, _: &AddNewlineAbove, _w: &mut Window, cx: &mut Context<Self>) {
        apply_change(&mut self.editor, |text, selection| {
            Transaction::change_by_selection(text, selection, |range| {
                let line = text.char_to_line(range.cursor(text.slice(..)));
                let start = text.line_to_char(line);
                (start, start, Some(Tendril::from("\n")))
            })
        });
        cx.notify();
    }
    pub fn join_selections(&mut self, _: &JoinSelections, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let mut changes = Vec::new();
        for range in selection.iter() {
            let start_line = text.char_to_line(range.from());
            let end_line = text.char_to_line(range.to());
            for line in start_line..end_line {
                let end = helix_core::line_ending::line_end_char_index(&text.slice(..), line);
                let next_start = text.line_to_char(line + 1);
                // Find first non-whitespace on next line
                let ws_end = text.line(line + 1).first_non_whitespace_char()
                    .map(|off| next_start + off)
                    .unwrap_or(next_start);
                changes.push((end, ws_end, Some(Tendril::from(" "))));
            }
        }
        if !changes.is_empty() {
            let transaction = Transaction::change(text, changes.into_iter());
            doc.apply(&transaction, view.id);
        }
        cx.notify();
    }
    pub fn join_selections_space(&mut self, _: &JoinSelectionsSpace, w: &mut Window, cx: &mut Context<Self>) {
        self.join_selections(&JoinSelections, w, cx);
    }

    // ── Indentation ──────────────────────────────────────────────────────────

    pub fn indent_cmd(&mut self, _: &Indent, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let indent_str = doc.indent_style.as_str();
        let mut changes = Vec::new();
        let mut lines_done = std::collections::HashSet::new();
        for range in selection.iter() {
            let line = text.char_to_line(range.cursor(text.slice(..)));
            if lines_done.insert(line) {
                let start = text.line_to_char(line);
                changes.push((start, start, Some(Tendril::from(indent_str))));
            }
        }
        if !changes.is_empty() {
            let transaction = Transaction::change(text, changes.into_iter());
            doc.apply(&transaction, view.id);
        }
        cx.notify();
    }
    pub fn unindent_cmd(&mut self, _: &Unindent, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let indent_width = doc.indent_style.indent_width(doc.tab_width());
        let mut changes = Vec::new();
        let mut lines_done = std::collections::HashSet::new();
        for range in selection.iter() {
            let line = text.char_to_line(range.cursor(text.slice(..)));
            if lines_done.insert(line) {
                let start = text.line_to_char(line);
                let line_text = text.line(line);
                let mut removed = 0;
                for ch in line_text.chars() {
                    if removed >= indent_width { break; }
                    match ch {
                        ' ' => removed += 1,
                        '\t' => { removed += indent_width; break; }
                        _ => break,
                    }
                }
                if removed > 0 {
                    changes.push((start, start + removed, None));
                }
            }
        }
        if !changes.is_empty() {
            let transaction = Transaction::change(text, changes.into_iter());
            doc.apply(&transaction, view.id);
        }
        cx.notify();
    }

    // ── Comments ─────────────────────────────────────────────────────────────

    pub fn toggle_comments(&mut self, _: &ToggleComments, _w: &mut Window, cx: &mut Context<Self>) {
        // Simplified: toggle "// " at line starts
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let mut lines = std::collections::BTreeSet::new();
        for range in selection.iter() {
            let start = text.char_to_line(range.from());
            let end = text.char_to_line(range.to());
            for line in start..=end { lines.insert(line); }
        }
        // Check if all lines are already commented
        let all_commented = lines.iter().all(|&line| {
            let line_text: String = text.line(line).to_string();
            line_text.trim_start().starts_with("//")
        });
        let mut changes: Vec<(usize, usize, Option<Tendril>)> = Vec::new();
        for &line in &lines {
            let start = text.line_to_char(line);
            let line_text: String = text.line(line).to_string();
            if all_commented {
                // Remove comment
                if let Some(pos) = line_text.find("// ") {
                    changes.push((start + pos, start + pos + 3, None));
                } else if let Some(pos) = line_text.find("//") {
                    changes.push((start + pos, start + pos + 2, None));
                }
            } else {
                // Add comment
                let indent_end = text.line(line).first_non_whitespace_char()
                    .map(|off| start + off)
                    .unwrap_or(start);
                changes.push((indent_end, indent_end, Some(Tendril::from("// "))));
            }
        }
        if !changes.is_empty() {
            let transaction = Transaction::change(text, changes.into_iter());
            doc.apply(&transaction, view.id);
        }
        cx.notify();
    }
    pub fn toggle_line_comments(&mut self, _: &ToggleLineComments, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_comments(&ToggleComments, w, cx);
    }
    pub fn toggle_block_comments(&mut self, _: &ToggleBlockComments, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_comments(&ToggleComments, w, cx); // simplified fallback
    }

    // ── Undo/Redo ────────────────────────────────────────────────────────────

    pub fn undo(&mut self, _: &Undo, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.undo(view);
        cx.notify();
    }
    pub fn redo(&mut self, _: &Redo, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.redo(view);
        cx.notify();
    }
    pub fn earlier(&mut self, _: &Earlier, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.earlier(view, helix_core::history::UndoKind::Steps(1));
        cx.notify();
    }
    pub fn later(&mut self, _: &Later, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        doc.later(view, helix_core::history::UndoKind::Steps(1));
        cx.notify();
    }

    // ── Selection management ─────────────────────────────────────────────────

    pub fn select_all(&mut self, _: &SelectAll, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let end = doc.text().len_chars();
        doc.set_selection(view.id, Selection::single(0, end));
        cx.notify();
    }
    pub fn collapse_selection(&mut self, _: &CollapseSelection, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let pos = range.cursor(text);
            Range::new(pos, pos)
        });
        cx.notify();
    }
    pub fn flip_selections(&mut self, _: &FlipSelections, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let selection = doc.selection(view.id).clone().transform(|range| {
            Range::new(range.head, range.anchor)
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    pub fn ensure_selections_forward(&mut self, _: &EnsureSelectionsForward, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id).clone().transform(|range| {
            if range.anchor > range.head { Range::new(range.head, range.anchor) } else { range }
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }
    pub fn keep_primary_selection(&mut self, _: &KeepPrimarySelection, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let primary = doc.selection(view.id).primary();
        doc.set_selection(view.id, Selection::single(primary.anchor, primary.head));
        cx.notify();
    }
    pub fn remove_primary_selection(&mut self, _: &RemovePrimarySelection, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let sel = doc.selection(view.id);
        if sel.len() > 1 {
            let idx = sel.primary_index();
            let ranges: Vec<_> = sel.iter().enumerate()
                .filter(|(i, _)| *i != idx)
                .map(|(_, r)| *r)
                .collect();
            doc.set_selection(view.id, Selection::new(ranges.into(), 0));
        }
        cx.notify();
    }
    pub fn extend_line(&mut self, _: &ExtendLine, _w: &mut Window, cx: &mut Context<Self>) {
        apply_motion(&mut self.editor, |text, range| {
            let line = text.char_to_line(range.cursor(text));
            let start = text.line_to_char(line);
            let end = if line + 1 < text.len_lines() { text.line_to_char(line + 1) } else { text.len_chars() };
            Range::new(start, end)
        });
        cx.notify();
    }
    pub fn extend_line_below(&mut self, _: &ExtendLineBelow, _w: &mut Window, cx: &mut Context<Self>) {
        self.extend_line(&ExtendLine, _w, cx);
    }
    pub fn extend_line_above(&mut self, _: &ExtendLineAbove, _w: &mut Window, cx: &mut Context<Self>) {
        self.extend_line(&ExtendLine, _w, cx);
    }
    pub fn select_current_line(&mut self, _: &SelectCurrentLine, _w: &mut Window, cx: &mut Context<Self>) {
        self.extend_line(&ExtendLine, _w, cx);
    }
    pub fn extend_to_line_bounds(&mut self, _: &ExtendToLineBounds, _w: &mut Window, cx: &mut Context<Self>) {
        self.extend_line(&ExtendLine, _w, cx);
    }
    pub fn shrink_to_line_bounds(&mut self, _: &ShrinkToLineBounds, _w: &mut Window, cx: &mut Context<Self>) {
        self.extend_line(&ExtendLine, _w, cx);
    }
    pub fn split_selection_on_newline(&mut self, _: &SplitSelectionOnNewline, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text();
        let selection = doc.selection(view.id);
        let mut ranges = Vec::new();
        for range in selection.iter() {
            let text_slice = text.slice(..);
            let from = range.from();
            let to = range.to();
            let mut last = from;
            for i in from..to {
                if text_slice.char(i) == '\n' {
                    if i > last { ranges.push(Range::new(last, i)); }
                    last = i + 1;
                }
            }
            if last < to { ranges.push(Range::new(last, to)); }
            if ranges.is_empty() { ranges.push(*range); }
        }
        doc.set_selection(view.id, Selection::new(ranges.into(), 0));
        cx.notify();
    }
    pub fn merge_selections(&mut self, _: &MergeSelections, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let sel = doc.selection(view.id);
        if sel.len() > 1 {
            let first = sel.iter().next().unwrap();
            let last = sel.iter().last().unwrap();
            let merged = Range::new(first.from(), last.to());
            doc.set_selection(view.id, Selection::single(merged.anchor, merged.head));
        }
        cx.notify();
    }
    pub fn merge_consecutive_selections(&mut self, _: &MergeConsecutiveSelections, w: &mut Window, cx: &mut Context<Self>) {
        self.merge_selections(&MergeSelections, w, cx);
    }
    pub fn rotate_selections_forward(&mut self, _: &RotateSelectionsForward, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let sel = doc.selection(view.id);
        if sel.len() > 1 {
            let new_idx = (sel.primary_index() + 1) % sel.len();
            let ranges: Vec<_> = sel.iter().cloned().collect();
            let new_sel = Selection::new(ranges.into(), new_idx);
            doc.set_selection(view.id, new_sel);
        }
        cx.notify();
    }
    pub fn rotate_selections_backward(&mut self, _: &RotateSelectionsBackward, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let sel = doc.selection(view.id);
        if sel.len() > 1 {
            let new_idx = if sel.primary_index() == 0 { sel.len() - 1 } else { sel.primary_index() - 1 };
            let ranges: Vec<_> = sel.iter().cloned().collect();
            let new_sel = Selection::new(ranges.into(), new_idx);
            doc.set_selection(view.id, new_sel);
        }
        cx.notify();
    }

    // ── Yank/Paste ───────────────────────────────────────────────────────────

    pub fn yank(&mut self, _: &Yank, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current_ref!(self.editor);
        let text = doc.text();
        let values: Vec<String> = doc.selection(view.id).iter()
            .map(|range| text.slice(range.from()..range.to()).to_string())
            .collect();
        let joined = values.join("\n");
        self.editor.registers.write('"', vec![joined]);
        cx.notify();
    }
    pub fn yank_to_clipboard(&mut self, _: &YankToClipboard, w: &mut Window, cx: &mut Context<Self>) {
        self.yank(&Yank, w, cx);
    }
    pub fn yank_main_selection_to_clipboard(&mut self, _: &YankMainSelectionToClipboard, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current_ref!(self.editor);
        let text = doc.text();
        let primary = doc.selection(view.id).primary();
        let val = text.slice(primary.from()..primary.to()).to_string();
        self.editor.registers.write('"', vec![val]);
        cx.notify();
    }
    pub fn paste_after(&mut self, _: &PasteAfter, _w: &mut Window, cx: &mut Context<Self>) {
        let text_to_paste = self.read_register('"');
        if let Some(text_to_paste) = text_to_paste {
            let (view, doc) = current!(self.editor);
            let text = doc.text();
            let selection = doc.selection(view.id);
            let transaction = Transaction::change_by_selection(text, selection, |range| {
                let pos = graphemes::next_grapheme_boundary(text.slice(..), range.cursor(text.slice(..)));
                (pos, pos, Some(Tendril::from(text_to_paste.as_str())))
            });
            doc.apply(&transaction, view.id);
        }
        cx.notify();
    }
    pub fn paste_before(&mut self, _: &PasteBefore, _w: &mut Window, cx: &mut Context<Self>) {
        let text_to_paste = self.read_register('"');
        if let Some(text_to_paste) = text_to_paste {
            let (view, doc) = current!(self.editor);
            let text = doc.text();
            let selection = doc.selection(view.id);
            let transaction = Transaction::change_by_selection(text, selection, |range| {
                let pos = range.cursor(text.slice(..));
                (pos, pos, Some(Tendril::from(text_to_paste.as_str())))
            });
            doc.apply(&transaction, view.id);
        }
        cx.notify();
    }
    pub fn paste_clipboard_after(&mut self, _: &PasteClipboardAfter, w: &mut Window, cx: &mut Context<Self>) {
        self.paste_after(&PasteAfter, w, cx);
    }
    pub fn paste_clipboard_before(&mut self, _: &PasteClipboardBefore, w: &mut Window, cx: &mut Context<Self>) {
        self.paste_before(&PasteBefore, w, cx);
    }
    pub fn replace_with_yanked(&mut self, _: &ReplaceWithYanked, _w: &mut Window, cx: &mut Context<Self>) {
        let text_to_paste = self.read_register('"');
        if let Some(text_to_paste) = text_to_paste {
            apply_change(&mut self.editor, |text, selection| {
                Transaction::change_by_selection(text, selection, |range| {
                    (range.from(), range.to(), Some(Tendril::from(text_to_paste.as_str())))
                })
            });
        }
        cx.notify();
    }
    pub fn replace_selections_with_clipboard(&mut self, _: &ReplaceSelectionsWithClipboard, w: &mut Window, cx: &mut Context<Self>) {
        self.replace_with_yanked(&ReplaceWithYanked, w, cx);
    }

    // ── Scroll/View ──────────────────────────────────────────────────────────

    pub fn page_up(&mut self, _: &PageUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(-24); cx.notify();
    }
    pub fn page_down(&mut self, _: &PageDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(24); cx.notify();
    }
    pub fn half_page_up(&mut self, _: &HalfPageUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(-12); cx.notify();
    }
    pub fn half_page_down(&mut self, _: &HalfPageDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(12); cx.notify();
    }
    pub fn scroll_up(&mut self, _: &ScrollUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(-3); cx.notify();
    }
    pub fn scroll_down(&mut self, _: &ScrollDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.scroll_lines(3); cx.notify();
    }

    pub fn align_view_middle(&mut self, _: &AlignViewMiddle, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn align_view_top(&mut self, _: &AlignViewTop, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn align_view_center(&mut self, _: &AlignViewCenter, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn align_view_bottom(&mut self, _: &AlignViewBottom, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }

    // ── Match brackets ───────────────────────────────────────────────────────

    pub fn match_brackets(&mut self, _: &MatchBrackets, _w: &mut Window, cx: &mut Context<Self>) {
        let (view, doc) = current!(self.editor);
        let text = doc.text().slice(..);
        let syntax = doc.syntax();
        let selection = doc.selection(view.id).clone().transform(|range| {
            if let Some(syntax) = syntax {
                if let Some(pos) = helix_core::match_brackets::find_matching_bracket(syntax, text, range.cursor(text)) {
                    return range.put_cursor(text, pos, false);
                }
            }
            range
        });
        doc.set_selection(view.id, selection);
        cx.notify();
    }

    // ── Increment/Decrement ──────────────────────────────────────────────────

    pub fn increment(&mut self, _: &Increment, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn decrement(&mut self, _: &Decrement, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }

    // ── Jumps ────────────────────────────────────────────────────────────────

    pub fn jump_forward(&mut self, _: &JumpForward, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn jump_backward(&mut self, _: &JumpBackward, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn save_selection(&mut self, _: &SaveSelection, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }

    // ── Stubs for commands needing more infrastructure ────────────────────────

    pub fn search(&mut self, _: &Search, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn rsearch(&mut self, _: &Rsearch, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn search_next(&mut self, _: &SearchNext, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn search_prev(&mut self, _: &SearchPrev, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn extend_search_next(&mut self, _: &ExtendSearchNext, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn extend_search_prev(&mut self, _: &ExtendSearchPrev, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn search_selection(&mut self, _: &SearchSelection, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn find_next_char(&mut self, _: &FindNextChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn find_till_char(&mut self, _: &FindTillChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn find_prev_char(&mut self, _: &FindPrevChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn till_prev_char(&mut self, _: &TillPrevChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn extend_next_char(&mut self, _: &ExtendNextChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn extend_till_char(&mut self, _: &ExtendTillChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn extend_prev_char(&mut self, _: &ExtendPrevChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn extend_till_prev_char(&mut self, _: &ExtendTillPrevChar, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn repeat_last_motion(&mut self, _: &RepeatLastMotion, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn command_mode(&mut self, _: &CommandMode, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn replace(&mut self, _: &Replace, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn surround_add(&mut self, _: &SurroundAdd, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn surround_replace(&mut self, _: &SurroundReplace, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn surround_delete(&mut self, _: &SurroundDelete, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn select_textobject_around(&mut self, _: &SelectTextobjectAround, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn select_textobject_inner(&mut self, _: &SelectTextobjectInner, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn expand_selection(&mut self, _: &ExpandSelection, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn shrink_selection(&mut self, _: &ShrinkSelection, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn select_next_sibling(&mut self, _: &SelectNextSibling, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn select_prev_sibling(&mut self, _: &SelectPrevSibling, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn goto_window_top(&mut self, _: &GotoWindowTop, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn goto_window_center(&mut self, _: &GotoWindowCenter, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn goto_window_bottom(&mut self, _: &GotoWindowBottom, _w: &mut Window, cx: &mut Context<Self>) { cx.notify(); }
    pub fn no_op(&mut self, _: &NoOp, _w: &mut Window, _cx: &mut Context<Self>) {}

    // ── Insert mode character input ──────────────────────────────────────────

    pub fn handle_key_down(&mut self, event: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.editor.mode != Mode::Insert { return; }
        if let Some(key_char) = &event.keystroke.key_char {
            if event.keystroke.modifiers.control || event.keystroke.modifiers.platform { return; }
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

    // ── Scrolling helper ─────────────────────────────────────────────────────

    fn read_register(&self, reg: char) -> Option<String> {
        self.editor.registers.read(reg, &self.editor)
            .map(|values| values.into_iter().collect::<Vec<_>>().join("\n"))
    }

    pub fn scroll_lines(&mut self, delta: i32) {
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
