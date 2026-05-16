//! Error pattern: infallible UI surface — failures are logged via `log::warn!`, never propagated. The single fallible call site (action lookup) logs and drops the keystroke.
//!
//! Sticky pane-resize overlay.
//!
//! After the user presses one of the `cmd-k shift-{h,j,k,l}` chords,
//! this transient modal mounts itself and keeps the workspace in a
//! resize-only key context for a short window (see [`STICKY_TIMEOUT`]).
//! While it's up, bare `h/j/k/l` dispatch the matching
//! `vim::ResizePane*` action and reset the timer; any other key (or
//! the timer firing) dismisses the overlay and hands focus back to
//! whatever pane was active before. The user can pile up several
//! nudges with a single chord at the front, which is the whole point
//! of the feature.
//!
//! Re-entry is idempotent: pressing another resize chord while the
//! overlay is already mounted refreshes its timer instead of stacking
//! a second modal.

use std::time::Duration;

use gpui::{
    Context, DismissEvent, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
    KeyContext, KeyDownEvent, ParentElement, Render, SharedString, Styled, Task, Window, div,
};
use ui::{ActiveTheme, Color, Label, LabelCommon, LabelSize};
use workspace::{ModalView, Workspace};

const STICKY_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug)]
pub enum ResizeDir {
    Left,
    Down,
    Up,
    Right,
}

impl ResizeDir {
    fn action_name(self) -> &'static str {
        match self {
            ResizeDir::Left => "vim::ResizePaneLeft",
            ResizeDir::Down => "vim::ResizePaneDown",
            ResizeDir::Up => "vim::ResizePaneUp",
            ResizeDir::Right => "vim::ResizePaneRight",
        }
    }
}

fn key_to_dir(key: &str) -> Option<ResizeDir> {
    // Normalize uppercase (some keyboards report `H` when shift is
    // held instead of `h` with the shift modifier).
    let key = match key.chars().count() {
        1 => key.chars().next().map(|c| c.to_ascii_lowercase()),
        _ => None,
    }?;
    match key {
        'h' => Some(ResizeDir::Left),
        'j' => Some(ResizeDir::Down),
        'k' => Some(ResizeDir::Up),
        'l' => Some(ResizeDir::Right),
        _ => None,
    }
}

pub struct ResizeStickyOverlay {
    focus_handle: FocusHandle,
    dismissed: bool,
    // Held to keep the inflight timer cancellable: dropping the task
    // cancels the await, which is how `arm_timer` resets the window
    // on every successful nudge.
    timeout_task: Option<Task<()>>,
}

impl ResizeStickyOverlay {
    /// Entry point — wired up from each `codon_session::ResizePane*`
    /// action handler. Always performs the requested nudge; opens the
    /// overlay (or refreshes its timer if already mounted) so any
    /// follow-up bare h/j/k/l keep resizing.
    pub fn arm(
        dir: ResizeDir,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        dispatch_resize(dir, window, cx);
        if let Some(existing) = workspace.active_modal::<ResizeStickyOverlay>(cx) {
            existing.update(cx, |this, cx| this.arm_timer(cx));
            return;
        }
        workspace.toggle_modal(window, cx, |_window, cx| ResizeStickyOverlay::new(cx));
    }

    fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            dismissed: false,
            timeout_task: None,
        };
        this.arm_timer(cx);
        this
    }

    fn arm_timer(&mut self, cx: &mut Context<Self>) {
        // Replace the inflight task — drop-cancels the previous one so
        // the dismiss only fires `STICKY_TIMEOUT` after the most
        // recent keystroke.
        self.timeout_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STICKY_TIMEOUT).await;
            this.update(cx, |this, cx| this.dismiss(cx)).ok();
        }));
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.dismiss(cx);
            return;
        }
        // Bare modifier press (just `shift`) arrives as a key-down too
        // — ignore so holding shift before the next hjkl doesn't kill
        // the overlay.
        if matches!(key, "shift" | "control" | "alt" | "platform" | "cmd") {
            return;
        }
        let mods = &event.keystroke.modifiers;
        // Anything with cmd/ctrl/alt is the user moving on (a fresh
        // chord, a save, a paste) — dismiss so we don't intercept it.
        if mods.platform || mods.control || mods.alt {
            self.dismiss(cx);
            return;
        }
        let Some(dir) = key_to_dir(key) else {
            self.dismiss(cx);
            return;
        };
        dispatch_resize(dir, window, cx);
        self.arm_timer(cx);
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        self.dismissed = true;
        self.timeout_task = None;
        cx.emit(DismissEvent);
    }
}

fn dispatch_resize(dir: ResizeDir, window: &mut Window, cx: &mut gpui::App) {
    match cx.build_action(dir.action_name(), None) {
        Ok(action) => window.dispatch_action(action, cx),
        Err(err) => log::warn!(
            "codon-session: could not build {} for sticky resize: {err:?}",
            dir.action_name()
        ),
    }
}

impl EventEmitter<DismissEvent> for ResizeStickyOverlay {}

impl Focusable for ResizeStickyOverlay {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for ResizeStickyOverlay {
    fn render_bare(&self) -> bool {
        // Paint our own absolute-positioned chip; skip ModalLayer's
        // centered-box treatment.
        true
    }
}

impl Render for ResizeStickyOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::default();
        key_context.add("ResizeStickyOverlay");

        let theme = cx.theme();
        let chip_bg = theme.colors().version_control_conflict;
        let chip_fg = theme.colors().text;

        let root = div()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .occlude()
            .absolute()
            .inset_0()
            .size_full();

        if self.dismissed {
            return root;
        }

        let chip = div()
            .absolute()
            .bottom_4()
            .right_4()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(chip_bg)
            .text_color(chip_fg)
            .child(
                Label::new(SharedString::from("↔ resize  h j k l  ·  esc"))
                    .size(LabelSize::Small)
                    .color(Color::Default),
            );

        root.child(chip)
    }
}
