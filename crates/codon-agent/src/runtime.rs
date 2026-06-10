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
pub fn reload_from_toml(cx: &mut App, content: &str) {
    AgentRegistry::reset_to_defaults(cx);
    let overrides = config::parse(content);
    config::apply_overrides(cx, overrides);
    config::apply_harness_settings(cx, content);
}
