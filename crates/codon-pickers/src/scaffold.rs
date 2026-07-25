//! Shared modal scaffolding for codon's keyboard-driven modals.
//!
//! Every codon modal (`CheatsheetModal`, `CommandPaletteModal`,
//! `SessionPicker`, `WindowPicker`, `DirPicker`, …) repeats the same
//! tiny dance: own a `FocusHandle`, implement `Focusable` by forwarding
//! to it, implement `EventEmitter<DismissEvent>` (a zero-body impl),
//! and — only for the command palette — toggle the global
//! `CodonModeTracker.command_active` flag while the modal is open so
//! the status-bar mode indicator can show `CMD`.
//!
//! `ModalScaffold` encapsulates the first part (focus handle + the
//! mode-tracker toggle) so each modal can hold it by composition
//! rather than re-implement the boilerplate inline. The
//! `EventEmitter<DismissEvent>` impl stays per-struct because it's a
//! zero-body marker trait — sharing it would add machinery without
//! removing duplication.
//!
//! Modals declare whether they participate in the mode-indicator
//! dance via [`ModalModeTag`]:
//!
//! - [`ModalModeTag::CommandActive`] — sets
//!   `CodonModeTracker.command_active = true` while open and clears
//!   it on dismiss. Used by the command palette.
//! - [`ModalModeTag::Inert`] — does not touch the mode tracker. Used
//!   by every other codon modal: the cheatsheet, the session/window
//!   pickers, and the directory picker leave the underlying pane's
//!   mode indicator alone.

use codon_mode::CodonModeTracker;
use gpui::{App, BorrowAppContext, Context, FocusHandle};

/// Whether the modal participates in the global
/// [`CodonModeTracker`] mode indicator. Stored verbatim on each
/// scaffold so the choice is visible at the callsite that constructs
/// the modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalModeTag {
    /// Toggle `CodonModeTracker.command_active = true` while the
    /// modal is open. The status-bar mode indicator reads this flag
    /// and forces the `COMMAND` label regardless of which pane is
    /// focused underneath the modal — semantically appropriate only
    /// for the command palette.
    CommandActive,
    /// Leave `CodonModeTracker` untouched. The status bar continues
    /// to reflect whichever pane was focused before the modal
    /// opened, which is the right behaviour for non-palette modals
    /// (cheatsheet, session picker, directory picker, …).
    Inert,
}

/// Shared modal boilerplate. Each codon modal owns one of these by
/// composition and forwards `Focusable::focus_handle` to
/// [`Self::focus_handle`]. The construction site calls
/// [`Self::on_open`] / [`Self::on_dismiss`] to bracket the lifetime
/// of the modal — for `CommandActive` modals this drives the mode
/// indicator; for `Inert` modals both calls are no-ops.
pub struct ModalScaffold {
    focus_handle: FocusHandle,
    mode_tag: ModalModeTag,
}

impl ModalScaffold {
    /// Construct a scaffold inside the modal's own `Context<Self>`.
    /// The freshly minted `FocusHandle` is owned by the scaffold and
    /// re-used by the modal's `Focusable` impl.
    ///
    /// The bound is `Sized + 'static` on the modal type so the
    /// caller can pass its own `cx: &mut Context<MyModal>`. Construction
    /// does not yet flip the mode tracker — call [`Self::on_open`]
    /// after the modal is wired up.
    pub fn new<T>(cx: &mut Context<T>, mode_tag: ModalModeTag) -> Self
    where
        T: Sized + 'static,
    {
        Self {
            focus_handle: cx.focus_handle(),
            mode_tag,
        }
    }

    /// Borrow the scaffold's focus handle. The modal's `Focusable`
    /// impl returns `self.focus_handle().clone()`.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Run when the modal opens. For [`ModalModeTag::CommandActive`]
    /// this sets `CodonModeTracker.command_active = true`. Inert
    /// modals do nothing.
    pub fn on_open(&self, cx: &mut App) {
        if matches!(self.mode_tag, ModalModeTag::CommandActive) {
            set_command_active(cx, true);
        }
    }

    /// Run when the modal dismisses. For
    /// [`ModalModeTag::CommandActive`] this clears
    /// `CodonModeTracker.command_active`. Inert modals do nothing.
    /// Idempotent — safe to call from `on_release` even if a prior
    /// dismissal path already cleared the flag.
    pub fn on_dismiss(&self, cx: &mut App) {
        if matches!(self.mode_tag, ModalModeTag::CommandActive) {
            set_command_active(cx, false);
        }
    }

    /// Inspect the configured mode tag. Useful in tests and for
    /// callers that want to assert their construction-time choice
    /// survived round-tripping through the scaffold.
    pub fn mode_tag(&self) -> ModalModeTag {
        self.mode_tag
    }
}

fn set_command_active(cx: &mut App, active: bool) {
    cx.update_global::<CodonModeTracker, _>(|tracker, _| {
        tracker.command_active = active;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use codon_mode::CodonModeTracker;
    use gpui::{AppContext as _, TestAppContext};

    /// Helper: a host entity for `Context<T>`. The scaffold's
    /// `new` requires a `Context<T>` for some `T: Sized + 'static`,
    /// so we need *some* type to instantiate as the host — its
    /// contents are irrelevant.
    struct TestHost;

    fn init(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(CodonModeTracker::default());
        });
    }

    #[gpui::test]
    async fn command_active_scaffold_toggles_global_flag(cx: &mut TestAppContext) {
        init(cx);

        let host = cx.new(|_| TestHost);
        let scaffold = host.update(cx, |_, cx| {
            ModalScaffold::new(cx, ModalModeTag::CommandActive)
        });

        cx.update(|cx| {
            assert!(!cx.global::<CodonModeTracker>().command_active);
            scaffold.on_open(cx);
            assert!(cx.global::<CodonModeTracker>().command_active);
            scaffold.on_dismiss(cx);
            assert!(!cx.global::<CodonModeTracker>().command_active);
        });
    }

    #[gpui::test]
    async fn inert_scaffold_does_not_touch_global_flag(cx: &mut TestAppContext) {
        init(cx);

        let host = cx.new(|_| TestHost);
        let scaffold = host.update(cx, |_, cx| ModalScaffold::new(cx, ModalModeTag::Inert));

        cx.update(|cx| {
            // Pre-seed the flag to a non-default value so we can
            // distinguish "scaffold leaves it alone" from "scaffold
            // never set it in the first place".
            cx.update_global::<CodonModeTracker, _>(|t, _| {
                t.command_active = true;
            });
            scaffold.on_open(cx);
            assert!(cx.global::<CodonModeTracker>().command_active);
            scaffold.on_dismiss(cx);
            assert!(cx.global::<CodonModeTracker>().command_active);
        });

        assert_eq!(scaffold.mode_tag(), ModalModeTag::Inert);
    }
}
