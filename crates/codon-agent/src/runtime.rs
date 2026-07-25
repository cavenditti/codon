//! Codon's agent runtime. One abstraction —
//! `Agent { model, system_prompt, tools }` — under which every
//! existing flow converges (the cross-pane verbs, the fish `#@`
//! completer, contextual-suggest when it lands). New flows pick a
//! model + prompt + tool set, register an [`Agent`], and call
//! [`Agent::run`].
//!
//! Tools may themselves be agents — see [`delegate::DelegateTool`] —
//! so a planner agent can offload work to a specialised worker
//! without bespoke orchestration code.
//!
//! Configuration: every built-in agent's `model`, `system_prompt`,
//! `temperature`, etc. is overridable from `codon.toml`'s
//! `[agent.<name>]` table. See [`config`].

pub mod agent;
pub mod cancel;
pub mod config;
pub mod delegate;
pub mod error;
pub mod event;
pub mod model;
pub mod registry;
pub mod routing;
pub mod safety;
pub mod tool;
pub mod trace;

pub use agent::{Agent, AgentBuilder, TurnOutcome};
pub use cancel::CancelToken;
pub use config::HarnessSettings;
pub use delegate::DelegateTool;
pub use error::{AgentError, ToolError};
pub use event::AgentEvent;
pub use model::{
    ModelClient, ModelSpec, ZedModelClient, pick_zed_model, start_provider_authentication,
    wait_for_provider_authentication,
};
pub use registry::AgentRegistry;
pub use routing::{RoutingFlow, RoutingFlowError, ShellCommandTool};
pub use safety::{SafetyDecision, SafetySource, SafetyVerdict, ShellPermissionRule};
pub use tool::{Tool, ToolSet};
pub use trace::{PhaseEvent, TRACE_TURN_CAP, ToolEvent, TraceLog, TraceOutcome, TurnTrace};

use gpui::App;

/// Install the [`AgentRegistry`] + [`TraceLog`] globals and register
/// every built-in agent. Idempotent — safe to call from `apps/codon`
/// init order alongside the other crate `init`s.
pub fn init(cx: &mut App) {
    AgentRegistry::install(cx);
    TraceLog::install(cx);
    crate::agents::register_builtins(cx);
}

/// Re-apply `[agent.*]` + `[agent_harness]` settings from an in-memory
/// `codon.toml`. Called on first load and from the config watcher on
/// every reload. Resets the registry to built-in defaults first so an
/// override the user *removed* stops applying without a restart.
///
/// A document that fails to parse is treated as a transient edit: the
/// current registry — built-in overrides *and* the last-good scripted
/// flow — is left untouched and the failure is surfaced as metadata
/// only, so a momentary syntax error never wipes live agents
/// (REQ:codon/agent-routing-harness#c-last-good). The document is
/// deserialized exactly once and the parsed tables are threaded into
/// routing, the `[agent.*]` merge, and the harness-settings global.
pub fn reload_from_toml(cx: &mut App, content: &str) {
    let table = match config::parse_document(content) {
        Ok(table) => table,
        Err(err) => {
            log::warn!(
                "codon-agent: codon.toml did not parse; keeping the last-good agent registry: {err}"
            );
            AgentRegistry::set_routing_error(cx, format!("codon.toml parse error: {err}"));
            return;
        }
    };
    AgentRegistry::reset_to_defaults(cx);
    routing::reload_from_harness_settings(cx, &table.agent_harness);
    config::apply_overrides(cx, table.agent);
    config::apply_harness_settings(cx, &table.agent_harness);
}
