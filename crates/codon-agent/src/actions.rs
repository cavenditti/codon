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
    let Some(panel) = workspace.panel::<AgentPanel>(cx) else {
        log::warn!("agent panel not registered; ignoring agent::Explain");
        return;
    };
    if !panel.focus_handle(cx).contains_focused(window, cx) {
        workspace.toggle_panel_focus::<AgentPanel>(window, cx);
    }
    panel.update(cx, |panel, cx| {
        panel.seed_explain_with_selection(Some(prefix.to_string()), window, cx);
    });
}
