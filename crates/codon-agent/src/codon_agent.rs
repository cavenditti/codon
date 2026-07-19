//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

pub mod actions;
pub mod agents;
pub mod runtime;
pub mod token_status;
pub mod trace_viewer;

pub use actions::*;
pub use runtime::{
    Agent, AgentBuilder, AgentError, AgentEvent, AgentRegistry, CancelToken, DelegateTool,
    HarnessSettings, ModelClient, ModelSpec, RoutingFlow, RoutingFlowError, ShellCommandTool, Tool,
    ToolError, ToolSet, TraceLog, TurnOutcome, TurnTrace, ZedModelClient,
    wait_for_provider_authentication,
};
pub use token_status::TokenStatusItem;

use gpui::App;

pub fn init(cx: &mut App) {
    runtime::init(cx);
    actions::register(cx);
    // NOTE: codon deliberately does NOT eagerly authenticate every
    // language-model provider at startup. That pass (added for the
    // out-of-the-box `#@` flow) cascades the whole proprietary AI
    // stack to life during launch — provider auth, ACP agent-registry
    // network fetches, copilot polling — and was a major contributor
    // to the release-build startup hang. Provider auth now happens
    // lazily on first agent use. See the phase-22 vendor-agnostic
    // agent-layer rework. `wait_for_provider_authentication` becomes a
    // no-op when the startup pass never ran.
}

/// Re-apply `[agent.*]` overrides from an in-memory `codon.toml`.
/// Exposed for `apps/codon` to call after `codon-config` loads the
/// user's TOML, and again from the config watcher on every change.
pub fn reload_from_toml(cx: &mut App, content: &str) {
    runtime::reload_from_toml(cx, content);
}
