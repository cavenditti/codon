use std::time::{Duration, Instant};

use gpui::{Context, Element, Entity, FontWeight, Render, SharedString, Subscription, WeakEntity, Window};
use ui::prelude::*;
use workspace::{StatusItemView, item::ItemHandle};

use codon_pane_bridge::{CodonModeTracker, PaneMode};
use vim::{Vim, VimEvent};

/// Duration of the status-bar colour flash that signals a keystroke
/// dead-end (unmapped key, or chord-prefix timeout with no bound
/// continuation). Matches the ~200 ms acceptance criterion in
/// `TASK:phase-20/dead-end-flash`.
const DEAD_END_FLASH: Duration = Duration::from_millis(200);

pub struct CodonModeIndicator {
    vim: Option<WeakEntity<Vim>>,
    vim_focused: bool,
    vim_subscription: Option<Subscription>,
    _tracker_subscription: Subscription,
    /// Subscription to `window.observe_keystroke_dead_end`. The vendored
    /// Zed matcher fires this when a keystroke produces no action (fresh
    /// unmapped, or chord-prefix timeout with no completion).
    _dead_end_subscription: Subscription,
    /// `Some(deadline)` while the dead-end flash is active. A second
    /// dead-end received before the deadline extends it to a fresh
    /// 200 ms window (coalesced — held keys do not strobe). `None`
    /// once the most recent flash has elapsed.
    flash_until: Option<Instant>,
}

impl CodonModeIndicator {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let _tracker_subscription =
            cx.observe_global::<CodonModeTracker>(|_: &mut Self, cx| cx.notify());

        let _dead_end_subscription =
            cx.observe_keystroke_dead_end(window, |this: &mut Self, _window, cx| {
                this.arm_dead_end_flash(cx);
            });

        let handle = cx.entity();
        let window_handle = window.window_handle();
        cx.observe_new::<Vim>(move |_, window, cx| {
            let Some(window) = window else {
                return;
            };
            if window.window_handle() != window_handle {
                return;
            }
            let vim = cx.entity();
            handle
                .update(cx, |_, cx| {
                    cx.subscribe(&vim, |indicator, vim, event, cx| match event {
                        VimEvent::Focused => {
                            indicator.vim_focused = true;
                            indicator.vim_subscription =
                                Some(cx.observe(&vim, |_, _, cx| cx.notify()));
                            indicator.vim = Some(vim.downgrade());
                        }
                    })
                    .detach()
                })
        })
        .detach();

        Self {
            vim: None,
            vim_focused: false,
            vim_subscription: None,
            _tracker_subscription,
            _dead_end_subscription,
            flash_until: None,
        }
    }

    /// Extend (or start) the dead-end flash. Always pushes the deadline
    /// to `now + DEAD_END_FLASH`, so a held key keeps the flash on
    /// without strobing. Schedules a single repaint at the deadline so
    /// `render` clears `flash_until` and drops back to the normal
    /// background.
    fn arm_dead_end_flash(&mut self, cx: &mut Context<Self>) {
        self.flash_until = Some(Instant::now() + DEAD_END_FLASH);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEAD_END_FLASH).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(deadline) = this.flash_until
                    && deadline <= Instant::now()
                {
                    this.flash_until = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn vim(&self) -> Option<Entity<Vim>> {
        self.vim.as_ref().and_then(|vim| vim.upgrade())
    }

    fn mode_from_vim(mode: vim::state::Mode) -> PaneMode {
        match mode {
            vim::state::Mode::Insert | vim::state::Mode::Replace => PaneMode::Insert,
            _ => PaneMode::Normal,
        }
    }

    fn detail_from_vim(mode: vim::state::Mode) -> Option<SharedString> {
        match mode {
            vim::state::Mode::Normal
            | vim::state::Mode::HelixNormal
            | vim::state::Mode::Insert => None,
            _ => Some(mode.to_string().into()),
        }
    }
}

impl Render for CodonModeIndicator {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tracker = cx.global::<CodonModeTracker>();
        let command_active = tracker.command_active;

        let (pane_mode, detail, temp_mode) = if command_active {
            (PaneMode::Command, None, false)
        } else if self.vim_focused && let Some(vim) = self.vim() {
            let vim_readable = vim.read(cx);
            let mode = vim_readable.mode;
            let temp = vim_readable.temp_mode;
            let status_label = vim_readable.status_label.clone();

            if let Some(label) = status_label {
                (Self::mode_from_vim(mode), Some(label), temp)
            } else {
                (
                    Self::mode_from_vim(mode),
                    Self::detail_from_vim(mode),
                    temp,
                )
            }
        } else {
            (tracker.mode, tracker.detail.clone(), false)
        };

        // Short, glanceable codon pane-mode label. Vim sub-modes (Visual,
        // Replace, Operator-pending, …) come through `detail` and replace
        // the short label — they're the more specific signal when they're
        // active.
        let short: SharedString = match pane_mode {
            PaneMode::Normal => "NOR".into(),
            PaneMode::Insert => "INS".into(),
            PaneMode::Command => "CMD".into(),
        };
        let mode_label: SharedString = if let Some(detail) = detail {
            detail.to_uppercase().into()
        } else if temp_mode {
            format!("(INS) {}", short).into()
        } else {
            short
        };

        let theme = cx.theme();
        let colors = theme.colors();
        let status = theme.status();
        let (vim_fg, vim_bg) = match pane_mode {
            PaneMode::Normal => (
                colors.vim_helix_normal_foreground,
                colors.vim_helix_normal_background,
            ),
            PaneMode::Insert => (colors.vim_insert_foreground, colors.vim_insert_background),
            PaneMode::Command => (colors.vim_replace_foreground, colors.vim_replace_background),
        };

        let transparent = gpui::hsla(0.0, 0.0, 0.0, 0.0);
        let (fallback_bg, fallback_fg) = match pane_mode {
            PaneMode::Normal => (status.info_background, status.info),
            PaneMode::Insert => (status.success_background, status.success),
            PaneMode::Command => (status.warning_background, status.warning),
        };
        let bg = if vim_bg == transparent { fallback_bg } else { vim_bg };
        let fg = if vim_fg == transparent { fallback_fg } else { vim_fg };

        let flash_active = self
            .flash_until
            .is_some_and(|deadline| deadline > Instant::now());
        let flash_bg = if flash_active {
            Some(status.warning_background)
        } else {
            None
        };

        h_flex()
            .gap_1()
            .when_some(flash_bg, |el, bg| el.bg(bg).rounded_sm().px_1())
            .child(
                v_flex()
                    .px_2()
                    .h(ButtonSize::Default.rems())
                    .justify_center()
                    .rounded_sm()
                    .bg(bg)
                    .child(
                        Label::new(mode_label)
                            .line_height_style(LineHeightStyle::UiLabel)
                            .weight(FontWeight::BOLD)
                            .color(Color::Custom(fg)),
                    ),
            )
            .into_any()
    }
}

impl StatusItemView for CodonModeIndicator {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // When the active pane item changes, clear vim_focused.
        // If the new item has a Vim, VimEvent::Focused will fire and set it back.
        self.vim_focused = false;
        cx.notify();
    }
}
