//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

mod cheatsheet_modal;
mod keymap;
mod passthrough;

pub use cheatsheet_modal::{KeybindingsCheatsheetModal, ShowKeymap};
pub use keymap::{
    CuratedBinding, codon_default_bindings, codon_glance_verbs, codon_user_bindings,
    load_codon_keymap,
};
pub use passthrough::SendPrefixToFocus;

use gpui::{App, KeyContext, SharedString};
use ui::text_for_keystrokes;
use workspace::Workspace;

/// Em-dash placeholder rendered in place of a chord when an action is
/// not bound under the current context stack. The single
/// `chord_for_action` consumer below produces this value so every codon
/// UI surface that names a verb without a chord uses the same glyph —
/// keep this constant in sync with the spec
/// (`REQ:codon/discoverability#c-binding-hints-everywhere`).
pub const UNBOUND_CHORD_PLACEHOLDER: &str = "—";

/// Return the human-readable chord string (e.g. `"cmd-k w n"`) for an
/// action under the supplied context stack, or [`UNBOUND_CHORD_PLACEHOLDER`]
/// (an em-dash) if the action exists in the registry but has no
/// binding. Looks up bindings against the **live** GPUI keymap, so a
/// user rebind (via `~/.config/codon/codon.toml` reload) reflects on
/// the next render without restart.
///
/// `contexts` is the context-stack predicate that the calling surface
/// renders under (typically `window.context_stack()` for an
/// element-tree-attached surface, or `&[]` for a modal that doesn't
/// scope its rendering to a pane). An empty slice matches only
/// context-free bindings — pass a real stack from `Window` when the
/// surface lives inside a pane.
///
/// Implementation: builds the action via `cx.build_action(name, None)`
/// (returns the placeholder if the name is not registered, exactly
/// like the cheatsheet's missing-action branch), then walks the keymap
/// looking for the first binding whose predicate matches the supplied
/// context stack. Order matches `Keymap::bindings_for_action`'s
/// iteration order (insertion order, with user overrides last) — for
/// display, the last binding takes precedence, mirroring how the
/// cheatsheet's `collect_bindings` collapses duplicates.
pub fn chord_for_action(cx: &App, action_name: &str, contexts: &[KeyContext]) -> SharedString {
    let Ok(action) = cx.build_action(action_name, None) else {
        return SharedString::from(UNBOUND_CHORD_PLACEHOLDER);
    };

    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();

    // Walk in reverse so user overrides (added after defaults) win,
    // matching the precedence rule documented on
    // `Keymap::bindings_for_input`.
    let chosen = keymap
        .bindings_for_action(action.as_ref())
        .rev()
        .find(|binding| match binding.predicate() {
            None => true,
            Some(predicate) => {
                if contexts.is_empty() {
                    false
                } else {
                    predicate.depth_of(contexts).is_some()
                }
            }
        });

    let Some(binding) = chosen else {
        return SharedString::from(UNBOUND_CHORD_PLACEHOLDER);
    };

    let keystrokes: Vec<_> = binding
        .keystrokes()
        .iter()
        .map(|k| k.inner().to_owned())
        .collect();
    if keystrokes.is_empty() {
        return SharedString::from(UNBOUND_CHORD_PLACEHOLDER);
    }
    SharedString::from(text_for_keystrokes(&keystrokes, cx))
}

/// Install codon-keymap's App-level wiring. Must be called once at
/// app startup, after `codon_mode::install_pane_mode_dispatcher`.
///
/// This intentionally does **not** call [`load_codon_keymap`] — that
/// is driven by `codon-config`'s watcher (initial load + reload on
/// change), so wiring it here would double-load on startup.
pub fn init(cx: &mut App) {
    codon_mode::register_pane_mode_bridge::<KeybindingsCheatsheetModal>(cx);
}

/// Wire codon-keymap's workspace-scoped action handlers — the
/// cheatsheet modal (`ShowKeymap`) and the double-prefix passthrough
/// (`SendPrefixToFocus`). Called from
/// `apps/codon/src/zed.rs`'s `observe_new::<Workspace>` chain.
pub fn register_for_workspace(workspace: &mut Workspace) {
    cheatsheet_modal::register_for_workspace(workspace);
    passthrough::register_for_workspace(workspace);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{KeyBinding as GpuiKeyBinding, TestAppContext, actions};

    actions!(codon_keymap_chord_test, [TestVerb]);

    #[gpui::test]
    async fn chord_for_action_returns_em_dash_when_unbound(cx: &mut TestAppContext) {
        cx.update(|cx| {
            // Helper short-circuits via `build_action` for unknown names —
            // the registry returns `Err`, so the em-dash falls out.
            let s = chord_for_action(cx, "nonexistent::Action", &[]);
            assert_eq!(s.as_ref(), UNBOUND_CHORD_PLACEHOLDER);
        });
    }

    #[gpui::test]
    async fn chord_for_action_returns_em_dash_when_action_known_but_unbound(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            cx.bind_keys(Vec::<GpuiKeyBinding>::new());
            // Registers `TestVerb` in the action registry but no
            // keybinding maps to it — em-dash branch.
            let s = chord_for_action(cx, "codon_keymap_chord_test::TestVerb", &[]);
            assert_eq!(s.as_ref(), UNBOUND_CHORD_PLACEHOLDER);
        });
    }

    #[gpui::test]
    async fn chord_for_action_renders_bound_chord(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let binding = GpuiKeyBinding::new("ctrl-x b", TestVerb, None);
            cx.bind_keys(vec![binding]);
            let s = chord_for_action(cx, "codon_keymap_chord_test::TestVerb", &[]);
            // Exact glyph form depends on platform-style + vim-mode flag
            // (`text_for_keystrokes` formats per `PlatformStyle::platform`),
            // so just assert the chord contains the load-bearing key and
            // is *not* the em-dash placeholder. Anchoring on a substring
            // keeps the test stable across platforms and across changes to
            // `text_for_keystrokes`'s glyph table.
            assert_ne!(s.as_ref(), UNBOUND_CHORD_PLACEHOLDER);
            assert!(
                s.to_lowercase().contains('b'),
                "expected chord text to mention `b`, got `{s}`",
            );
        });
    }
}
