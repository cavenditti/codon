use std::io::{Read, Write};
use std::sync::Arc;

use alacritty_terminal::event::{Event as AlacEvent, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use alacritty_terminal::Term;
use gpui::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// Simple dimensions for terminal initialization.
struct TermDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone)]
struct JsonListener {
    pty_writer: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
}

impl EventListener for JsonListener {
    fn send_event(&self, event: AlacEvent) {
        if let AlacEvent::PtyWrite(text) = event {
            if let Some(mut writer) = self.pty_writer.try_lock() {
                let _ = writer.write_all(text.as_bytes());
                let _ = writer.flush();
            }
        }
    }
}

pub struct TerminalPane {
    term: Arc<FairMutex<Term<JsonListener>>>,
    pty_writer: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
    focus_handle: FocusHandle,
    _poll_task: gpui::Task<()>,
}

impl TerminalPane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cols = 80u16;
        let rows = 24u16;

        // Create PTY first (the listener needs the writer)
        let pty_system = native_pty_system();
        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty failed");

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-l");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "codon");

        let mut child = pty_pair
            .slave
            .spawn_command(cmd)
            .expect("spawn login shell failed");

        std::thread::spawn(move || {
            let _ = child.wait();
        });

        let mut pty_reader = pty_pair.master.try_clone_reader().expect("pty reader");
        let pty_writer = pty_pair.master.take_writer().expect("pty writer");
        let pty_writer: Arc<parking_lot::Mutex<Box<dyn Write + Send>>> =
            Arc::new(parking_lot::Mutex::new(pty_writer));

        // Create alacritty Term with a listener that writes responses back to PTY
        let config = TermConfig {
            scrolling_history: 10_000,
            ..Default::default()
        };
        let dims = TermDimensions {
            cols: cols as usize,
            rows: rows as usize,
        };
        let listener = JsonListener {
            pty_writer: pty_writer.clone(),
        };
        let term = Term::new(config, &dims, listener.clone());
        let term = Arc::new(FairMutex::new(term));

        // Reader thread: reads PTY output and feeds it to the alacritty Term
        let term_for_reader = term.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut parser = alacritty_terminal::vte::ansi::Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new();
            loop {
                let n = match pty_reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                let mut term_lock = term_for_reader.lock();
                parser.advance(&mut *term_lock, &buf[..n]);
            }
        });

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        // Poll for terminal updates at ~60fps
        let poll_task = cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });

        Self {
            term,
            pty_writer,
            focus_handle,
            _poll_task: poll_task,
        }
    }

    fn send_input(&self, bytes: &[u8]) {
        if let Some(mut writer) = self.pty_writer.try_lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }
}

impl Render for TerminalPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Collect cells while holding the lock, then release
        let cells: Vec<_> = {
            let term = self.term.lock_unfair();
            let content = term.renderable_content();
            let cells: Vec<_> = content
                .display_iter
                .map(|ic| (ic.point, ic.cell.clone()))
                .collect();

            cells
        };

        // Build lines with color runs
        let default_fg: Hsla = rgb(0xcdd6f4).into();
        let mut all_lines: Vec<Vec<(String, Hsla)>> = Vec::new();
        let mut current_line: i32 = i32::MIN;
        let mut run_text = String::new();
        let mut run_fg = default_fg;

        for (point, cell) in &cells {
            if point.line.0 != current_line {
                // Flush current run
                if !run_text.is_empty() {
                    if let Some(line) = all_lines.last_mut() {
                        line.push((std::mem::take(&mut run_text), run_fg));
                    }
                }
                // Start new line
                all_lines.push(Vec::new());
                current_line = point.line.0;
                run_fg = default_fg;
            }

            if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                continue;
            }

            let mut fg = cell.fg;
            let mut bg = cell.bg;
            if cell.flags.contains(CellFlags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let fg_color = ansi_to_gpui(&fg);

            // Color change -> flush run
            if fg_color != run_fg && !run_text.is_empty() {
                if let Some(line) = all_lines.last_mut() {
                    line.push((std::mem::take(&mut run_text), run_fg));
                }
            }
            run_fg = fg_color;
            run_text.push(cell.c);
        }
        // Flush final run
        if !run_text.is_empty() {
            if let Some(line) = all_lines.last_mut() {
                line.push((run_text, run_fg));
            }
        }

        // Trim trailing whitespace from each line's last run
        for line in &mut all_lines {
            if let Some(last_run) = line.last_mut() {
                let trimmed = last_run.0.trim_end().to_string();
                if trimmed.is_empty() {
                    line.pop();
                } else {
                    last_run.0 = trimmed;
                }
            }
            // Keep trimming if there are more trailing space-only runs
            while line.last().is_some_and(|(t, _)| t.trim().is_empty()) {
                line.pop();
            }
        }

        div()
            .id("terminal")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(0x1e1e2e))
            .font_family("Lilex")
            .text_size(px(14.0))
            .text_color(rgb(0xcdd6f4))
            .overflow_hidden()
            .whitespace_nowrap()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, _cx| {
                let bytes = keystroke_to_bytes(&event.keystroke);
                if !bytes.is_empty() {
                    this.send_input(&bytes);
                }
            }))
            .child(
                div().flex().flex_col().children(
                    all_lines.into_iter().map(|runs| {
                        if runs.is_empty() {
                            return div().child(SharedString::from(" "));
                        }
                        // Build a single string + highlight ranges for the line
                        let mut full_text = String::new();
                        let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
                        for (text, fg) in &runs {
                            let start = full_text.len();
                            full_text.push_str(text);
                            let end = full_text.len();
                            highlights.push((
                                start..end,
                                HighlightStyle {
                                    color: Some(fg.clone()),
                                    ..Default::default()
                                },
                            ));
                        }
                        if full_text.is_empty() {
                            full_text.push(' ');
                        }
                        let styled = StyledText::new(SharedString::from(full_text))
                            .with_default_highlights(&_window.text_style(), highlights);
                        div().child(styled)
                    }),
                ),
            )
    }
}

fn ansi_to_gpui(color: &AnsiColor) -> Hsla {
    match color {
        AnsiColor::Named(n) => match n {
            NamedColor::Black | NamedColor::DimBlack => rgb(0x45475a).into(),
            NamedColor::Red | NamedColor::DimRed => rgb(0xf38ba8).into(),
            NamedColor::Green | NamedColor::DimGreen => rgb(0xa6e3a1).into(),
            NamedColor::Yellow | NamedColor::DimYellow => rgb(0xf9e2af).into(),
            NamedColor::Blue | NamedColor::DimBlue => rgb(0x89b4fa).into(),
            NamedColor::Magenta | NamedColor::DimMagenta => rgb(0xf5c2e7).into(),
            NamedColor::Cyan | NamedColor::DimCyan => rgb(0x94e2d5).into(),
            NamedColor::White | NamedColor::DimWhite => rgb(0xbac2de).into(),
            NamedColor::BrightBlack => rgb(0x585b70).into(),
            NamedColor::BrightRed => rgb(0xf38ba8).into(),
            NamedColor::BrightGreen => rgb(0xa6e3a1).into(),
            NamedColor::BrightYellow => rgb(0xf9e2af).into(),
            NamedColor::BrightBlue => rgb(0x89b4fa).into(),
            NamedColor::BrightMagenta => rgb(0xf5c2e7).into(),
            NamedColor::BrightCyan => rgb(0x94e2d5).into(),
            NamedColor::BrightWhite => rgb(0xcdd6f4).into(),
            NamedColor::Foreground | NamedColor::Cursor => rgb(0xcdd6f4).into(),
            NamedColor::Background | NamedColor::DimForeground | NamedColor::BrightForeground => {
                rgb(0x1e1e2e).into()
            }
        },
        AnsiColor::Spec(rgb_val) => {
            let r = rgb_val.r as f32 / 255.0;
            let g = rgb_val.g as f32 / 255.0;
            let b = rgb_val.b as f32 / 255.0;
            Hsla::from(Rgba { r, g, b, a: 1.0 })
        }
        AnsiColor::Indexed(idx) => {
            if *idx < 16 {
                let named = match idx {
                    0 => NamedColor::Black,
                    1 => NamedColor::Red,
                    2 => NamedColor::Green,
                    3 => NamedColor::Yellow,
                    4 => NamedColor::Blue,
                    5 => NamedColor::Magenta,
                    6 => NamedColor::Cyan,
                    7 => NamedColor::White,
                    8 => NamedColor::BrightBlack,
                    9 => NamedColor::BrightRed,
                    10 => NamedColor::BrightGreen,
                    11 => NamedColor::BrightYellow,
                    12 => NamedColor::BrightBlue,
                    13 => NamedColor::BrightMagenta,
                    14 => NamedColor::BrightCyan,
                    15 => NamedColor::BrightWhite,
                    _ => unreachable!(),
                };
                ansi_to_gpui(&AnsiColor::Named(named))
            } else {
                rgb(0xcdd6f4).into()
            }
        }
    }
}

fn keystroke_to_bytes(keystroke: &Keystroke) -> Vec<u8> {
    if let Some(key_char) = &keystroke.key_char {
        if keystroke.modifiers.control && key_char.len() == 1 {
            let c = key_char.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                return vec![(c.to_ascii_lowercase() as u8) - b'a' + 1];
            }
        }

        match keystroke.key.as_ref() {
            "enter" => return vec![b'\r'],
            "tab" => return vec![b'\t'],
            "escape" => return vec![0x1b],
            "backspace" => return vec![0x7f],
            "delete" => return b"\x1b[3~".to_vec(),
            "up" => return b"\x1b[A".to_vec(),
            "down" => return b"\x1b[B".to_vec(),
            "right" => return b"\x1b[C".to_vec(),
            "left" => return b"\x1b[D".to_vec(),
            "home" => return b"\x1b[H".to_vec(),
            "end" => return b"\x1b[F".to_vec(),
            "pageup" => return b"\x1b[5~".to_vec(),
            "pagedown" => return b"\x1b[6~".to_vec(),
            _ => {}
        }

        return key_char.as_bytes().to_vec();
    }

    match keystroke.key.as_ref() {
        "enter" => vec![b'\r'],
        "tab" => vec![b'\t'],
        "escape" => vec![0x1b],
        "backspace" => vec![0x7f],
        "space" => vec![b' '],
        "delete" => b"\x1b[3~".to_vec(),
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "pageup" => b"\x1b[5~".to_vec(),
        "pagedown" => b"\x1b[6~".to_vec(),
        _ => vec![],
    }
}
