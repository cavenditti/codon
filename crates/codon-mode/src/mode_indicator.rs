use std::time::{Duration, Instant};

use gpui::{
    Context, Element, Entity, FontWeight, Render, SharedString, Subscription, Task, WeakEntity,
    Window,
};
use ui::prelude::*;
use workspace::{StatusItemView, item::ItemHandle};

use codon_pane_bridge::{CodonGlanceTable, CodonModeTracker, PaneMode};
use vim::{Vim, VimEvent, state::VimGlobals};

/// How long the glance lingers after a mode/focus transition before
/// fading. The spec calls for ~2 s and explicit cancel on the next
/// non-motion keypress (decay is the only path implemented in this
/// task; the keypress hook lands with the sibling
/// `TASK:phase-20/action-history-ring`).
const GLANCE_DECAY: Duration = Duration::from_millis(2000);

/// Duration of the status-bar colour flash that signals a keystroke
/// dead-end (unmapped key, or chord-prefix timeout with no bound
/// continuation). Matches the ~200 ms acceptance criterion in
/// `TASK:phase-20/dead-end-flash`.
const DEAD_END_FLASH: Duration = Duration::from_millis(200);

pub struct CodonModeIndicator {
    vim: Option<WeakEntity<Vim>>,
    vim_focused: bool,
    pending_keys: Option<String>,
    vim_subscription: Option<Subscription>,
    _tracker_subscription: Subscription,
    /// Per-pane curated verb hint surfaced in the status bar for ~2 s
    /// after every mode / focus transition. Cleared by the decay task
    /// (or, in a follow-up task, by the action-history hook on the
    /// next non-motion keypress).
    glance: GlanceState,
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

/// Status-bar glance state. `verbs` is empty when nothing is being
/// rendered; `last_key` lets us avoid re-arming the decay task for the
/// same (pane, mode) pair when the tracker re-publishes without an
/// actual transition (Vim, for instance, fires `cx.notify()` on every
/// observe even when its mode didn't change).
#[derive(Default)]
struct GlanceState {
    verbs: Vec<SharedString>,
    last_key: Option<String>,
    decay_task: Option<Task<()>>,
}

impl CodonModeIndicator {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_pending_input(window, |this: &mut Self, window, cx| {
            this.update_pending_keys(window, cx);
            cx.notify();
        })
        .detach();

        let _tracker_subscription = cx.observe_global::<CodonModeTracker>(|this: &mut Self, cx| {
            this.refresh_glance(cx);
            cx.notify();
        });

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
            pending_keys: None,
            vim_subscription: None,
            _tracker_subscription,
            glance: GlanceState::default(),
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
        // Always spawn a repaint at this deadline. Older tasks from
        // earlier dead-ends may fire first, but they only call
        // `cx.notify()` — `render` keeps `flash_until` set if it's
        // still in the future, so the visible flash naturally
        // extends to the most recent dead-end's window.
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

    /// Re-read the live mode tracker, decide the curated `(pane, mode)`
    /// key, and arm the glance if the pair changed since the last
    /// publish. No-op when the global glance table is empty (tests,
    /// codon-keymap hasn't loaded yet) or when the curated row is
    /// explicitly empty (user escape hatch).
    fn refresh_glance(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.glance_key(cx) else {
            self.glance.verbs.clear();
            self.glance.last_key = None;
            self.glance.decay_task = None;
            return;
        };
        if self.glance.last_key.as_deref() == Some(key.0.as_str()) {
            return;
        }
        let verbs = cx
            .try_global::<CodonGlanceTable>()
            .map(|table| table.verbs(&key.1, &key.2).to_vec())
            .unwrap_or_default();
        self.glance.last_key = Some(key.0);
        self.glance.verbs = verbs;
        if self.glance.verbs.is_empty() {
            self.glance.decay_task = None;
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(GLANCE_DECAY).await;
            let _ = this.update(cx, |this, cx| {
                this.glance.verbs.clear();
                this.glance.decay_task = None;
                cx.notify();
            });
        });
        self.glance.decay_task = Some(task);
    }

    /// Decide which curated `(pane, mode)` row to render. Returns
    /// `(full_key, pane, mode)` where `full_key` is the
    /// dedup-comparison value. Today only Normal-mode rows are
    /// curated; Insert and Command intentionally fall through to
    /// "no glance" so the status bar stays quiet while typing.
    fn glance_key(&self, cx: &mut Context<Self>) -> Option<(String, String, String)> {
        let tracker = cx.global::<CodonModeTracker>();
        if tracker.command_active {
            return None;
        }
        let mode = if self.vim_focused
            && let Some(vim) = self.vim()
        {
            Self::mode_from_vim(vim.read(cx).mode)
        } else {
            tracker.mode
        };
        let mode_str = match mode {
            PaneMode::Normal => "normal",
            PaneMode::Insert => "insert",
            PaneMode::Command => return None,
        };
        let pane = if self.vim_focused {
            "editor".to_string()
        } else if let Some(kind) = tracker.pane_kind.as_ref() {
            kind.to_string()
        } else {
            // No codon-aware pane has reported a kind yet; render no
            // glance. The dispatcher writes `pane_kind` on every
            // focus-in, so this branch is the boot-time gap (and
            // panes that haven't opted in to `pane_kind()` yet).
            return None;
        };
        Some((format!("{pane}.{mode_str}"), pane, mode_str.to_string()))
    }

    /// Public entry point for the future non-motion-keypress hook.
    /// Wipes the glance immediately; the decay task (if armed) is
    /// dropped and silently drops its `cx.notify()`.
    pub fn cancel_glance(&mut self, cx: &mut Context<Self>) {
        if self.glance.verbs.is_empty() {
            return;
        }
        self.glance.verbs.clear();
        self.glance.decay_task = None;
        cx.notify();
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

        // Glance — curated 3–5-verb hint rendered to the LEFT of the
        // mode label (the right edge of the status bar is reserved
        // for cursor position and other Zed-owned indicators). Pure
        // text, theme-secondary foreground, no background — must not
        // visually compete with the mode pill. See
        // REQ:codon/discoverability#c-status-bar-mode-glance.
        let glance_text: Option<SharedString> = if self.glance.verbs.is_empty() {
            None
        } else {
            let joined = self
                .glance
                .verbs
                .iter()
                .map(|v| v.as_ref())
                .collect::<Vec<_>>()
                .join("  ");
            Some(joined.into())
        };

        // Dead-end flash — a brief warning-tinted background on the
        // status-bar item when the keystroke matcher hit a terminal
        // empty state (unmapped key, or chord timeout without a bound
        // completion). Theme-aware: uses `status.warning_background`
        // which every theme guarantees is legible against the
        // surrounding status-bar surface.
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
            .when_some(glance_text, |el, text| {
                el.child(
                    Label::new(text)
                        .line_height_style(LineHeightStyle::UiLabel)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
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
        // Re-arm the glance — focus changes are a "transition" per
        // REQ:codon/discoverability#c-status-bar-mode-glance even
        // when the pane mode itself didn't change.
        self.glance.last_key = None;
        self.refresh_glance(cx);
        cx.notify();
    }
}
