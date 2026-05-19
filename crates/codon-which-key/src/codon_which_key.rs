//! Codon which-key chord overlay.
//!
//! Replaces vendored Zed's `which_key` crate with a HUD that:
//!
//! - spans the full width of the **active pane** (not the bottom-right
//!   480 px of the OS window) — [`REQ:codon/which-key-overlay#c-full-pane-width`].
//! - flows possible bindings across multiple columns sized to
//!   `pane_width / min_column_width` — [`REQ:codon/which-key-overlay#c-multi-column`].
//! - prefixes the pending-keys label with the current
//!   [`codon_mode::CodonModeTracker`] pane mode —
//!   [`REQ:codon/which-key-overlay#c-mode-aware-title`].
//! - auto-flips to the top edge when content would occlude more than
//!   `flip_threshold` of the pane — [`REQ:codon/which-key-overlay#c-auto-flip`].
//!
//! Settings are read from `[which_key]` in `~/.config/codon/codon.toml`;
//! see [`codon_which_key_settings`]. The pending-input plumbing is
//! copied verbatim from `vendor/zed/crates/which_key/which_key.rs`.
//!
//! `apps/codon/src/main.rs` must call [`init`] *instead of*
//! `which_key::init` — `REQ:codon/which-key-overlay#c-suppress-zed`.

pub mod codon_which_key_modal;
pub mod codon_which_key_settings;

use std::{sync::LazyLock, time::Duration};

use codon_which_key_modal::CodonWhichKeyModal;
use codon_which_key_settings::load as load_settings;
use gpui::{App, Keystroke};
use util::ResultExt;
use workspace::Workspace;

pub fn init(cx: &mut App) {
    cx.observe_new(|_: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        let mut timer = None;
        cx.observe_pending_input(window, move |workspace, window, cx| {
            if window.pending_input_keystrokes().is_none() {
                if let Some(modal) = workspace.active_modal::<CodonWhichKeyModal>(cx) {
                    modal.update(cx, |modal, cx| modal.dismiss(cx));
                }
                timer.take();
                return;
            }

            let settings = load_settings();
            if !settings.enabled {
                return;
            }

            let delay_ms = settings.delay_ms;
            let settings_for_modal = settings;

            timer.replace(cx.spawn_in(window, async move |workspace_handle, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(delay_ms))
                    .await;
                workspace_handle
                    .update_in(cx, |workspace, window, cx| {
                        if workspace.active_modal::<CodonWhichKeyModal>(cx).is_some() {
                            return;
                        }
                        workspace.toggle_modal(window, cx, |window, cx| {
                            CodonWhichKeyModal::new(
                                workspace_handle.clone(),
                                settings_for_modal,
                                window,
                                cx,
                            )
                        });
                    })
                    .log_err();
            }));
        })
        .detach();
    })
    .detach();
}

/// Hard-coded list of chord prefixes to suppress in the HUD.
///
/// Carried over verbatim from `vendor/zed/crates/which_key/which_key.rs`
/// — these are vim chord families where the duplicates would clutter the
/// HUD (`ctrl-w` chords each have a `ctrl-` variant that re-binds the
/// same action) or where the row would be redundant noise (`g j`, `g k`
/// are normal-mode motions that are not chord prefixes).
pub static FILTERED_KEYSTROKES: LazyLock<Vec<Vec<Keystroke>>> = LazyLock::new(|| {
    [
        "g j",
        "g k",
        "ctrl-w ctrl-a",
        "ctrl-w ctrl-c",
        "ctrl-w ctrl-h",
        "ctrl-w ctrl-j",
        "ctrl-w ctrl-k",
        "ctrl-w ctrl-l",
        "ctrl-w ctrl-n",
        "ctrl-w ctrl-o",
        "ctrl-w ctrl-p",
        "ctrl-w ctrl-q",
        "ctrl-w ctrl-s",
        "ctrl-w ctrl-v",
        "ctrl-w ctrl-w",
        "ctrl-w ctrl-]",
        "ctrl-w ctrl-shift-w",
        "ctrl-w ctrl-g t",
        "ctrl-w ctrl-g shift-t",
    ]
    .iter()
    .filter_map(|s| {
        let keystrokes: Result<Vec<_>, _> =
            s.split(' ').map(Keystroke::parse).collect();
        keystrokes.ok()
    })
    .collect()
});
