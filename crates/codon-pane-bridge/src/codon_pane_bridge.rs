//! Error pattern: no fallible APIs — this crate exposes only infallible bridge registrations and `PaneMode` state.
//!
//! Pane → mode-tracker dispatch primitives.
//!
//! This crate is the cycle-free home of:
//!
//! - [`PaneMode`] — the three-state enum the status bar reports.
//! - [`CodonModeTracker`] — the App global the indicator reads from.
//! - [`PaneModeBridge`] — the trait every codon pane / modal
//!   implements so the dispatcher knows how to translate "this
//!   entity is focused" into a tracker write.
//! - [`install_pane_mode_dispatcher`] + [`register_pane_mode_bridge`]
//!   — the dispatcher itself.
//!
//! The split out of `codon-mode` exists for a single reason: the
//! `codon-mode::mode_indicator` module reads `vim::state` to
//! translate Vim's per-pane mode into [`PaneMode`], and `vim`
//! transitively depends on `editor`, which depends on `codon-jump`.
//! If `codon-jump` (or anything else upstream of `editor` in Zed's
//! dep graph) needed to depend on `codon-mode` to impl the bridge
//! trait, we'd close a build-cycle.
//!
//! Keeping the trait + tracker types in this tiny vim-free crate
//! breaks that potential cycle. `codon-mode` depends on this crate
//! and re-exports the types so existing call sites don't notice.
//!
//! See `REQ:codon/code-quality#c-mode-dispatch-hook` and
//! `TASK:phase-14/mode-bridge-trait` for the rationale.

pub mod object_grammar;

pub use object_grammar::{
    AroundContainer, DiagnosticRef, GrammarKind, GrammarSelection, InnerContainer, ObjectGrammar,
    ObjectNext, ObjectPrev, SelectAll, TerminalBlockRef,
};

use gpui::{App, BorrowAppContext, Focusable, Global, SharedString, actions};

/// The three-state mode the status bar reports about the focused
/// pane. Mirrors Vim's coarse mode taxonomy so a user moving
/// between a terminal, a file-manager pane and an editor pane
/// reads a consistent label.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum PaneMode {
    #[default]
    Normal,
    Insert,
    Command,
}

impl std::fmt::Display for PaneMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneMode::Normal => write!(f, "NORMAL"),
            PaneMode::Insert => write!(f, "INSERT"),
            PaneMode::Command => write!(f, "COMMAND"),
        }
    }
}

/// App-global that the status-bar mode indicator reads from and
/// the pane-mode dispatcher writes to. Exactly one of these is
/// installed at app boot via the host crate's `init`.
pub struct CodonModeTracker {
    pub mode: PaneMode,
    pub detail: Option<SharedString>,
    /// True while a command-class modal (palette / jump overlay /
    /// etc.) is focused. The indicator forces `PaneMode::Command`
    /// whenever this is set, regardless of which pane or vim mode
    /// is otherwise focused — the modal owns the UI.
    pub command_active: bool,
    /// Snake-case identifier for the currently focused pane kind
    /// (`"editor"`, `"terminal"`, `"file_manager"`, `"git_panel"`,
    /// `"peek_dock"`). Used by the status-bar mode indicator to look
    /// up the curated glance verb row for the (pane, mode) pair on
    /// every transition (REQ:codon/discoverability#c-status-bar-mode-glance).
    /// `None` when no codon-aware pane is focused; the indicator then
    /// renders no glance.
    pub pane_kind: Option<SharedString>,
}

impl Default for CodonModeTracker {
    fn default() -> Self {
        Self {
            mode: PaneMode::Normal,
            detail: None,
            command_active: false,
            pane_kind: None,
        }
    }
}

impl Global for CodonModeTracker {}

/// Curated per-pane × per-mode glance verbs surfaced by the status-bar
/// mode indicator. Populated by `codon-keymap` at load time from the
/// embedded `[glance.*]` table (and user `~/.config/codon/codon.toml`
/// overrides); read by `codon-mode::mode_indicator` on every
/// `CodonModeTracker` change to render a brief 3–5 verb hint to the
/// right of the mode label.
///
/// Lives here (not in `codon-keymap`) to avoid a cyclic dep —
/// `codon-keymap` already deps on `codon-mode`, which deps on this
/// crate.
#[derive(Default, Clone)]
pub struct CodonGlanceTable {
    /// Map from `"{pane_kind}.{mode}"` (e.g. `"editor.normal"`) to the
    /// verb list. `pane_kind` matches the snake-case form used in the
    /// glance TOML (`editor`, `terminal`, `file_manager`, `git_panel`).
    /// An entry whose `Vec` is empty MUST hide the glance for that
    /// pane × mode (escape hatch per spec).
    pub entries: std::collections::HashMap<String, Vec<SharedString>>,
}

impl CodonGlanceTable {
    /// Look up the verbs for a given pane × mode. Returns an empty
    /// slice if no entry exists for the pair.
    pub fn verbs(&self, pane_kind: &str, mode: &str) -> &[SharedString] {
        let key = format!("{pane_kind}.{mode}");
        self.entries
            .get(&key)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

impl Global for CodonGlanceTable {}

actions!(codon_mode, [SwitchToNormal, SwitchToInsert, SwitchToCommand]);

/// Implemented by every codon-owned pane / modal that participates
/// in the modal status-bar layer.
///
/// The dispatcher reads these two methods whenever the entity's
/// focus handle gains focus; pane authors do not call into
/// [`CodonModeTracker`] themselves.
pub trait PaneModeBridge: 'static {
    /// What `PaneMode` the status bar should show while this entity
    /// is focused.
    fn pane_mode(&self) -> PaneMode;

    /// Optional snake-case identifier for the pane kind backing this
    /// entity — `"editor"`, `"terminal"`, `"file_manager"`,
    /// `"git_panel"`, `"peek_dock"`. The dispatcher writes this into
    /// `CodonModeTracker::pane_kind` on focus-in so the status-bar
    /// mode indicator can look up the curated glance row for the
    /// `(pane, mode)` pair on every transition. `None` (the default)
    /// leaves the tracker's `pane_kind` untouched — the right call
    /// for command-class modals that don't represent a real pane.
    fn pane_kind(&self) -> Option<&'static str> {
        None
    }

    /// Optional override for the tracker's `command_active` flag,
    /// which forces the COMMAND label in the status bar regardless
    /// of `pane_mode()`.
    ///
    /// - `Some(true)` — set the flag while focused (typical for
    ///   command-class modals like the palette / jump overlay).
    /// - `Some(false)` — clear the flag on focus (used by modals
    ///   that explicitly want to *not* show COMMAND, e.g. the
    ///   cheatsheet, so they don't inherit it from a still-open
    ///   palette underneath).
    /// - `None` — leave `command_active` untouched (the common
    ///   case for non-modal panes).
    fn command_active_override(&self) -> Option<bool> {
        None
    }
}

/// Marker global proving [`install_pane_mode_dispatcher`] has run.
/// Used to make the install idempotent and to gate
/// [`register_pane_mode_bridge`] in debug builds.
#[derive(Default)]
struct PaneModeDispatcherInstalled;

impl Global for PaneModeDispatcherInstalled {}

/// Install the central pane-mode dispatcher into the app.
///
/// This is a tiny boot step: it just records that the dispatcher
/// machinery is live so subsequent [`register_pane_mode_bridge`]
/// calls have a place to attach their per-type observers. Calling
/// it twice is a no-op.
///
/// Call once at app startup, after the host has installed a
/// [`CodonModeTracker`] global for the dispatcher to write to.
pub fn install_pane_mode_dispatcher(cx: &mut App) {
    if cx.has_global::<PaneModeDispatcherInstalled>() {
        return;
    }
    cx.set_global(PaneModeDispatcherInstalled);
}

/// Register a pane type with the dispatcher.
///
/// For every newly created `Entity<T>`, the dispatcher hooks an
/// `on_focus_in` listener that reads the entity's [`PaneModeBridge`]
/// impl and pushes the result into [`CodonModeTracker`]. The
/// listener is scoped to the entity's `Context<T>`, so it's torn
/// down automatically when the entity is dropped.
///
/// Safe to call from a pane crate's `init(cx)` — typically directly
/// after the crate's other init wiring.
pub fn register_pane_mode_bridge<T>(cx: &mut App)
where
    T: PaneModeBridge + Focusable + 'static,
{
    debug_assert!(
        cx.has_global::<PaneModeDispatcherInstalled>(),
        "register_pane_mode_bridge::<{}> called before \
         install_pane_mode_dispatcher",
        std::any::type_name::<T>(),
    );

    // `observe_new` fires once per newly-created `Entity<T>` and
    // gives us the entity's `Context<T>` plus a chance at the
    // active `Window`. We use the window (when present) to register
    // an `on_focus_in` listener targeted at the entity's own focus
    // handle; that listener does the actual tracker push.
    //
    // Entities created without a window (test plumbing, headless
    // setups) just don't get a focus listener — they couldn't
    // receive keyboard focus anyway. The dispatcher fall-through
    // also writes the tracker once on creation when the entity is
    // *already* focused (the common case: a pane that focuses
    // itself in `new`), so the status bar reflects the pane mode
    // immediately even before any focus event fires.
    cx.observe_new::<T>(|entity, window, cx| {
        apply_bridge(entity, cx);
        // Modals that set `command_active = true` on focus would
        // otherwise leak that flag past their own lifetime — the
        // dispatcher pairs the focus-in apply with a release hook
        // that clears the flag back to false when the entity is
        // dropped. Non-command bridges (override is None or
        // Some(false)) need no release cleanup.
        if matches!(entity.command_active_override(), Some(true)) {
            cx.on_release(|_this, cx| {
                cx.update_global::<CodonModeTracker, _>(|tracker, _| {
                    tracker.command_active = false;
                });
            })
            .detach();
        }
        let Some(window) = window else {
            return;
        };
        let focus_handle = entity.focus_handle(cx);
        cx.on_focus_in(&focus_handle, window, |this, _window, cx| {
            apply_bridge(this, cx);
        })
        .detach();
    })
    .detach();
}

/// Push the bridge's declared mode + command-active override into
/// the global tracker. Shared between the focus-in listener and the
/// initial creation-time apply.
fn apply_bridge<T>(bridge: &T, cx: &mut App)
where
    T: PaneModeBridge,
{
    let mode = bridge.pane_mode();
    let command_override = bridge.command_active_override();
    let pane_kind = bridge.pane_kind();
    cx.update_global::<CodonModeTracker, _>(|tracker, _| {
        tracker.mode = mode;
        tracker.detail = None;
        if let Some(active) = command_override {
            tracker.command_active = active;
        }
        if let Some(kind) = pane_kind {
            tracker.pane_kind = Some(SharedString::from(kind));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Empty, FocusHandle, IntoElement, Render, TestAppContext, Window};

    /// A Normal-mode pane that returns `None` for the command-active
    /// override — the dispatcher should set mode = Normal but leave
    /// command_active untouched.
    struct NormalPane {
        focus_handle: FocusHandle,
    }

    impl NormalPane {
        fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                focus_handle: cx.focus_handle(),
            }
        }
    }

    impl gpui::Focusable for NormalPane {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for NormalPane {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }

    impl PaneModeBridge for NormalPane {
        fn pane_mode(&self) -> PaneMode {
            PaneMode::Normal
        }
    }

    /// A command-class modal — sets `command_active = true` on focus
    /// and clears it on drop.
    struct CommandModal {
        focus_handle: FocusHandle,
    }

    impl CommandModal {
        fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
            Self {
                focus_handle: cx.focus_handle(),
            }
        }
    }

    impl gpui::Focusable for CommandModal {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Render for CommandModal {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }

    impl PaneModeBridge for CommandModal {
        fn pane_mode(&self) -> PaneMode {
            PaneMode::Normal
        }

        fn command_active_override(&self) -> Option<bool> {
            Some(true)
        }
    }

    /// Construct the dispatcher, register two pane kinds, create
    /// instances of each (which drives the dispatcher's
    /// creation-time apply — same code path as the focus-in
    /// listener), and assert the tracker transitions match what
    /// each bridge declares.
    ///
    /// Focus-event simulation in `TestAppContext` requires a real
    /// draw cycle to materialise the focus path, which we don't
    /// have in a headless unit test. The creation-time apply uses
    /// the same [`apply_bridge`] entry point as the focus-in
    /// listener, so this still covers the dispatcher's tracker
    /// contract for both `Option<bool>` settings.
    #[gpui::test]
    fn dispatcher_pushes_bridge_state_on_creation(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(CodonModeTracker::default());
            install_pane_mode_dispatcher(cx);
            register_pane_mode_bridge::<NormalPane>(cx);
            register_pane_mode_bridge::<CommandModal>(cx);
        });

        cx.update(|cx| {
            cx.update_global::<CodonModeTracker, _>(|tracker, _| {
                tracker.mode = PaneMode::Insert;
                tracker.command_active = true;
            });
        });

        let _normal = cx.add_window(NormalPane::new);
        cx.run_until_parked();
        cx.update(|cx| {
            let tracker = cx.global::<CodonModeTracker>();
            assert_eq!(
                tracker.mode,
                PaneMode::Normal,
                "Normal pane creation should drive tracker.mode = Normal",
            );
            assert!(
                tracker.command_active,
                "command_active_override = None must leave the flag untouched",
            );
        });

        cx.update(|cx| {
            cx.update_global::<CodonModeTracker, _>(|tracker, _| {
                tracker.command_active = false;
            });
        });
        let _command = cx.add_window(CommandModal::new);
        cx.run_until_parked();
        cx.update(|cx| {
            let tracker = cx.global::<CodonModeTracker>();
            assert!(
                tracker.command_active,
                "command_active_override = Some(true) must set the flag",
            );
        });
    }
}
