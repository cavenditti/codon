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
    panel.update(cx, |panel, cx| {
        panel.seed_explain_with_selection(Some(prefix.to_string()), window, cx);
    });
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
