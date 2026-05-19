use agent_ui::AgentPanel;
use command_palette_hooks::{ActionAcceptsRegistry, ObjectKind};
use gpui::{App, Context, Focusable as _, Window, actions};
use workspace::Workspace;

actions!(
    codon_agent,
    [
        /// Send the current selection (text, file, hunk, etc.) to the agent
        /// prefixed with "Please explain this:".
        AgentExplain,
        /// Send the current selection to the agent prefixed with
        /// "Please summarize:".
        AgentSummarize,
        /// Send the current selection to the agent prefixed with
        /// "Please refactor this code, keeping behavior identical:".
        AgentRefactor,
    ]
);

const EXPLAIN_PREFIX: &str = "Please explain this:\n\n";
const SUMMARIZE_PREFIX: &str = "Please summarize:\n\n";
const REFACTOR_PREFIX: &str = "Please refactor this code, keeping behavior identical:\n\n";

const SELECTION_OBJECT_KINDS: &[ObjectKind] = &[
    ObjectKind::Text,
    ObjectKind::File,
    ObjectKind::Dir,
    ObjectKind::Hunk,
    ObjectKind::Diagnostic,
    ObjectKind::Block,
];

pub fn register(cx: &mut App) {
    let registry = cx.global_mut::<ActionAcceptsRegistry>();
    registry.register::<AgentExplain>(SELECTION_OBJECT_KINDS);
    registry.register::<AgentSummarize>(SELECTION_OBJECT_KINDS);
    registry.register::<AgentRefactor>(SELECTION_OBJECT_KINDS);
}

pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_agent_explain);
    workspace.register_action(handle_agent_summarize);
    workspace.register_action(handle_agent_refactor);
}

fn handle_agent_explain(
    workspace: &mut Workspace,
    _: &AgentExplain,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    seed_agent(workspace, EXPLAIN_PREFIX, window, cx);
}

fn handle_agent_summarize(
    workspace: &mut Workspace,
    _: &AgentSummarize,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    seed_agent(workspace, SUMMARIZE_PREFIX, window, cx);
}

fn handle_agent_refactor(
    workspace: &mut Workspace,
    _: &AgentRefactor,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    seed_agent(workspace, REFACTOR_PREFIX, window, cx);
}

fn seed_agent(
    workspace: &mut Workspace,
    prefix: &'static str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // Phase 12: the agent panel is reached via `codon_panes::OpenAgent`
    // and lives as a `PanelItemAdapter<AgentPanel>` item in a pane,
    // *not* a dock entry. `Workspace::panel::<AgentPanel>` only returns
    // dock entries, so fall back to walking the workspace's pane tree
    // for an existing adapter-hosted panel before logging a "not
    // registered" warning.
    let panel = workspace
        .panel::<AgentPanel>(cx)
        .or_else(|| find_adapter_hosted_agent(workspace, cx));
    let Some(panel) = panel else {
        log::warn!("agent panel not registered; ignoring agent::Explain");
        return;
    };
    if !panel.focus_handle(cx).contains_focused(window, cx) {
        // Try the dock-focus path first (works when peek-mounted); if no
        // such dock entry exists, fall back to focusing the panel's own
        // focus handle, which routes the adapter pane into view.
        if workspace.panel::<AgentPanel>(cx).is_some() {
            workspace.toggle_panel_focus::<AgentPanel>(window, cx);
        } else {
            panel.focus_handle(cx).focus(window, cx);
        }
    }
    // Phase-19: when a `"<char>` prefix has armed a register, expand
    // that register's contents into the seed string before calling
    // `seed_explain_with_selection`. The agent panel still calls
    // `insert_selections` for whatever the focused pane reports
    // (Helix-text-register interop is the follow-up task); this hook
    // just gets the register name + a one-line summary into the seed
    // so the user can see *which* register the verb is consuming.
    let resolved_prefix = resolve_register_prefix(prefix, cx);
    panel.update(cx, |panel, cx| {
        panel.seed_explain_with_selection(Some(resolved_prefix), window, cx);
    });
}

/// Resolve any armed `"<char>` register into a human-readable line
/// appended to the seed prefix. Single-file registers print the path
/// inline; multi-file registers print the count and let the agent
/// prompt enumerate them on a follow-up turn.
///
/// Non-`Files` registers / no-arming returns the prefix unchanged so
/// the path is fully no-op-friendly. Helix-text-register interop and
/// full selection-injection are out of scope for this task — see
/// `phase-19/selection-registers-helix-interop`.
fn resolve_register_prefix(prefix: &str, cx: &gpui::App) -> String {
    let store = codon_session::RegisterStore::global(cx);
    let Some(pending) = store.take_pending() else {
        return prefix.to_string();
    };
    let Some(selection) = store.read(pending.name) else {
        log::debug!(
            "codon-agent: register '{}' is empty; proceeding with focused selection",
            pending.name
        );
        return prefix.to_string();
    };
    let codon_mode::Selection::Files(paths) = selection else {
        log::debug!(
            "codon-agent: register '{}' does not hold files; proceeding with focused selection",
            pending.name
        );
        return prefix.to_string();
    };
    if paths.is_empty() {
        return prefix.to_string();
    }
    let mut buf = String::from(prefix);
    buf.push_str("[register \"");
    buf.push(pending.name.as_char());
    buf.push_str("\" — ");
    if paths.len() == 1 {
        buf.push_str(&paths[0].display().to_string());
    } else {
        buf.push_str(&format!("{} files", paths.len()));
    }
    buf.push_str("]\n\n");
    buf
}

/// Walk every pane in the workspace looking for a `PanelItemAdapter<AgentPanel>`
/// item, returning the inner panel entity if found. Used after Phase 12 dock
/// deprecation, when the agent panel only exists as an adapter-hosted item.
fn find_adapter_hosted_agent(
    workspace: &Workspace,
    cx: &gpui::App,
) -> Option<gpui::Entity<AgentPanel>> {
    workspace
        .panes()
        .iter()
        .flat_map(|pane| pane.read(cx).items())
        .find_map(|item| {
            item.act_as::<AgentPanel>(cx)
                .or_else(|| item.downcast::<AgentPanel>())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_prefix_ends_with_blank_line_separator() {
        // The trailing `\n\n` separates the codon-supplied verb from the
        // user's selected text so the agent reads it as a block, not a
        // run-on sentence.
        assert!(EXPLAIN_PREFIX.ends_with("\n\n"));
        assert!(SUMMARIZE_PREFIX.ends_with("\n\n"));
        assert!(REFACTOR_PREFIX.ends_with("\n\n"));
    }

    #[test]
    fn prefixes_are_distinct() {
        // Each verb has a distinct prompt — guards against a copy-paste
        // mistake silently making two verbs identical.
        assert_ne!(EXPLAIN_PREFIX, SUMMARIZE_PREFIX);
        assert_ne!(EXPLAIN_PREFIX, REFACTOR_PREFIX);
        assert_ne!(SUMMARIZE_PREFIX, REFACTOR_PREFIX);
    }

    #[test]
    fn refactor_prefix_pins_behavior_invariant() {
        // Codon's refactor verb explicitly promises the agent will keep
        // behaviour identical — losing that phrase would change product
        // semantics, so pin it from a test.
        assert!(REFACTOR_PREFIX.contains("keeping behavior identical"));
    }

    #[test]
    fn selection_object_kinds_cover_expected_objects() {
        // The agent seed actions accept Text, File, Dir, Hunk, Diagnostic,
        // and Block selections. Removing one would silently break a
        // documented cross-pane verb pairing.
        assert!(SELECTION_OBJECT_KINDS.contains(&ObjectKind::Text));
        assert!(SELECTION_OBJECT_KINDS.contains(&ObjectKind::File));
        assert!(SELECTION_OBJECT_KINDS.contains(&ObjectKind::Dir));
        assert!(SELECTION_OBJECT_KINDS.contains(&ObjectKind::Hunk));
        assert!(SELECTION_OBJECT_KINDS.contains(&ObjectKind::Diagnostic));
        assert!(SELECTION_OBJECT_KINDS.contains(&ObjectKind::Block));
    }
}
