//! Events emitted by `Agent::run` while a turn is in flight. The
//! consumer subscribes to a `Receiver<AgentEvent>` and the agent
//! drives one of these per state transition. Final outcome is
//! returned out-of-band via the run future's `Result<TurnOutcome>`.

use language_model::TokenUsage;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// The agent produced a partial text fragment in the current turn.
    /// Caller may stream this into a UI.
    Text(String),
    /// The model decided to call a tool. Emitted once the tool input
    /// has fully streamed in. The agent dispatches and feeds the
    /// result back on the next turn — this event is informational.
    ToolCall {
        name: Arc<str>,
        input: serde_json::Value,
    },
    /// The tool returned. `is_error` reflects fail-soft outcomes that
    /// were folded back into the model conversation.
    ToolResult {
        name: Arc<str>,
        is_error: bool,
        output: String,
    },
    /// Per-turn token-usage delta reported by the provider.
    Usage(TokenUsage),
}
