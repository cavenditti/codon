use gpui::{
    Context, Element, Entity, FontWeight, Render, SharedString, Subscription, WeakEntity, Window,
};
use ui::prelude::*;
use workspace::{StatusItemView, item::ItemHandle};

use codon_pane_bridge::{CodonModeTracker, PaneMode};
use vim::{Vim, VimEvent, state::VimGlobals};

pub struct CodonModeIndicator {
    vim: Option<WeakEntity<Vim>>,
    vim_focused: bool,
    pending_keys: Option<String>,
    vim_subscription: Option<Subscription>,
    _tracker_subscription: Subscription,
}

impl CodonModeIndicator {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_pending_input(window, |this: &mut Self, window, cx| {
            this.update_pending_keys(window, cx);
            cx.notify();
        })
        .detach();

        let _tracker_subscription = cx.observe_global::<CodonModeTracker>(|_, cx| {
            cx.notify();
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
            pending_keys: None,
            vim_subscription: None,
            _tracker_subscription,
        }
    }

    fn update_pending_keys(&mut self, window: &mut Window, cx: &gpui::App) {
        self.pending_keys = window
            .pending_input_keystrokes()
            .map(|keystrokes| ui::text_for_keystrokes(keystrokes, cx));
    }

    fn vim(&self) -> Option<Entity<Vim>> {
        self.vim.as_ref().and_then(|vim| vim.upgrade())
    }

    fn vim_pending_description(&self, cx: &mut Context<Self>) -> String {
        let globals = cx.global::<VimGlobals>();
        let mut parts = Vec::new();
        if let Some(reg) = globals.recording_register {
            parts.push(format!("recording @{reg}"));
        }
        if let Some(count) = globals.pre_count {
            parts.push(format!("{}", count));
        }
        if let Some(count) = globals.post_count {
            parts.push(format!("{}", count));
        }
        parts.join(" ")
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

        let (pane_mode, detail, pending, temp_mode) = if command_active {
            // Palette open — that's the whole UI right now; nothing else
            // (terminal vi mode, vim, focused pane) should override it.
            let pending = self.pending_keys.clone().unwrap_or_default();
            (PaneMode::Command, None, pending, false)
        } else if self.vim_focused && let Some(vim) = self.vim() {
            let vim_readable = vim.read(cx);
            let mode = vim_readable.mode;
            let temp = vim_readable.temp_mode;
            let status_label = vim_readable.status_label.clone();

            if let Some(label) = status_label {
                (Self::mode_from_vim(mode), Some(label), String::new(), temp)
            } else {
                let vim_pending = self.vim_pending_description(cx);
                let pending = self
                    .pending_keys
                    .clone()
                    .unwrap_or(vim_pending);
                (
                    Self::mode_from_vim(mode),
                    Self::detail_from_vim(mode),
                    pending,
                    temp,
                )
            }
        } else {
            let pending = self.pending_keys.clone().unwrap_or_default();
            (tracker.mode, tracker.detail.clone(), pending, false)
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
        // Per-mode saturated fallbacks pulled from `theme.status()` —
        // guaranteed defined in every theme. Maps to the vim convention
        // (Normal blue, Insert green, Command red/orange).
        let (fallback_bg, fallback_fg) = match pane_mode {
            PaneMode::Normal => (status.info_background, status.info),
            PaneMode::Insert => (status.success_background, status.success),
            PaneMode::Command => (status.warning_background, status.warning),
        };
        let bg = if vim_bg == transparent { fallback_bg } else { vim_bg };
        let fg = if vim_fg == transparent { fallback_fg } else { vim_fg };

        h_flex()
            .gap_1()
            .when(!pending.is_empty(), |el| {
                el.child(
                    Label::new(pending)
                        .line_height_style(LineHeightStyle::UiLabel)
                        .weight(FontWeight::MEDIUM),
                )
            })
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
