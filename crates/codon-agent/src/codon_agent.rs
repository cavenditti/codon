//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.

pub mod actions;
pub mod agents;
pub mod runtime;
pub mod token_status;
pub mod trace_viewer;

pub use actions::*;
pub use runtime::{
    Agent, AgentBuilder, AgentError, AgentEvent, AgentRegistry, CancelToken, DelegateTool,
    HarnessSettings, ModelClient, ModelSpec, Tool, ToolError, ToolSet, TraceLog, TurnOutcome,
    TurnTrace, ZedModelClient, wait_for_provider_authentication,
};
pub use token_status::TokenStatusItem;

use gpui::App;

pub fn init(cx: &mut App) {
    runtime::init(cx);
    actions::register(cx);
    // Zed defers provider authentication to the agent panel's first
    // load; codon agent flows must resolve a model without the panel
    // ever opening, so run the same pass at startup.
    runtime::start_provider_authentication(cx);
}

/// Re-apply `[agent.*]` overrides from an in-memory `codon.toml`.
/// Exposed for `apps/codon` to call after `codon-config` loads the
/// user's TOML, and again from the config watcher on every change.
pub fn reload_from_toml(cx: &mut App, content: &str) {
    runtime::reload_from_toml(cx, content);
}
