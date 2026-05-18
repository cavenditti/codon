//! Double-prefix passthrough — `prefix prefix` sends the literal
//! prefix keystroke to the focused terminal pane (tmux `send-prefix`).
//!
//! Bound by default to `"prefix prefix"` in `DEFAULT_KEYMAP`. The
//! handler resolves the user's configured prefix (the same value
//! `keymap::expand_prefix_in_bindings` uses), parses it as a single
//! [`gpui::Keystroke`], and dispatches it directly to the focused
//! [`TerminalView`]'s underlying [`terminal::Terminal`] via
//! `try_keystroke` — which is Alacritty's existing converter from a
//! GPUI keystroke into the PTY byte sequence the shell expects
//! (`ctrl-x` → `0x18`, etc.). The action is a silent no-op when
//! focus is not a terminal pane.
//!
//! The passthrough bypasses GPUI's keymap matcher (it writes straight
//! to the PTY entity), so there's no risk of the synthesized
//! keystroke re-matching the `prefix prefix` chord and recursing.

use gpui::{Context, Keystroke, Window, actions};
use settings::Settings as _;
use terminal::terminal_settings::TerminalSettings;
use terminal_view::TerminalView;
use workspace::Workspace;

use crate::keymap::resolve_prefix;

actions!(
    codon_keymap,
    [
        /// Forward the configured chord prefix to the focused terminal
        /// pane (tmux `send-prefix`). No-op outside a focused terminal.
        SendPrefixToFocus,
    ]
);

/// Wire the workspace-scoped passthrough handler. Call once per
/// workspace from the codon init chain.
pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_send_prefix_to_focus);
}

fn handle_send_prefix_to_focus(
    workspace: &mut Workspace,
    _: &SendPrefixToFocus,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let prefix_str = resolve_prefix();
    let keystroke = match Keystroke::parse(&prefix_str) {
        Ok(ks) => ks,
        Err(err) => {
            log::warn!(
                "SendPrefixToFocus: cannot parse resolved prefix '{prefix_str}' \
                 as a single keystroke: {err}. Set `[keymap] prefix = \"<chord>\"` \
                 to a single keystroke (e.g. \"ctrl-x\", \"ctrl-b\") to use \
                 double-tap passthrough."
            );
            return;
        }
    };

    let Some(item) = workspace.active_item(cx) else {
        log::trace!("SendPrefixToFocus: no active item, no-op");
        return;
    };
    let Some(view) = item.downcast::<TerminalView>() else {
        log::trace!("SendPrefixToFocus: active item is not a terminal, no-op");
        return;
    };

    let option_as_meta = TerminalSettings::get_global(cx).option_as_meta;
    view.update(cx, |tv, cx| {
        let terminal = tv.entity().clone();
        terminal.update(cx, |term, _| {
            if !term.try_keystroke(&keystroke, option_as_meta) {
                log::trace!(
                    "SendPrefixToFocus: terminal rejected keystroke '{prefix_str}' \
                     (modifier likely unsupported by Alacritty's converter)"
                );
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The resolved prefix must round-trip through `Keystroke::parse`
    /// for the passthrough to do anything useful. The fallback
    /// `DEFAULT_PREFIX = "cmd-k"` does parse — confirms the build-time
    /// default keeps the action wired (even though `cmd-k` is a poor
    /// prefix for passthrough, since terminals don't typically receive
    /// the `cmd` modifier).
    #[test]
    fn default_prefix_parses_as_keystroke() {
        let parsed = Keystroke::parse("cmd-k");
        assert!(parsed.is_ok(), "default prefix must parse as a keystroke");
    }

    /// A control-modifier prefix (the realistic tmux choice) parses
    /// and carries the expected modifier flag — try_keystroke depends
    /// on the modifier reaching Alacritty's converter.
    #[test]
    fn ctrl_x_prefix_parses_with_control_modifier() {
        let parsed = Keystroke::parse("ctrl-x").expect("ctrl-x parses");
        assert!(parsed.modifiers.control);
        assert_eq!(parsed.key, "x");
    }

    /// A multi-key chord string (e.g. a user setting
    /// `prefix = "ctrl-x ctrl-y"`) is rejected by `Keystroke::parse`,
    /// which is why the handler logs and bails — the spec only
    /// supports single-keystroke prefixes for passthrough.
    #[test]
    fn multi_keystroke_prefix_is_rejected() {
        // Multi-key chord strings aren't valid single-keystroke input.
        let parsed = Keystroke::parse("ctrl-x ctrl-y");
        assert!(parsed.is_err(), "multi-keystroke string must not parse as a single Keystroke");
    }
}
