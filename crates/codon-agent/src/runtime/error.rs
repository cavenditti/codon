//! Error pattern: bias toward `anyhow::Error` at the API boundary, but
//! the agent loop also distinguishes the soft "the model produced
//! something the tool layer recognised as invalid" path so callers can
//! decide whether to retry. Tool errors are *not* propagated — they're
//! folded back into the conversation as tool-result content so the
//! model can recover.

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("no language model is available — open the agent panel and configure one")]
    NoModelAvailable,
    #[error("agent `{0}` is not registered")]
    UnknownAgent(Arc<str>),
    #[error("turn was cancelled before it completed")]
    Cancelled,
    #[error("exceeded max_turns ({0})")]
    TooManyTurns(usize),
    #[error("language model error: {0}")]
    Model(#[from] language_model::LanguageModelCompletionError),
    #[error("language model produced an empty response")]
    EmptyResponse,
    #[error("{0:#}")]
    Other(#[from] anyhow::Error),
}

/// Reported by `Tool::run`. The agent loop folds this back into a
/// `tool_result` with `is_error = true` so the model can adapt.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool input did not match its schema: {0}")]
    BadInput(String),
    #[error("shell command denied by safety evaluator: {0}")]
    SafetyDenied(String),
    #[error("shell safety evaluator unavailable: {0}")]
    SafetyUnavailable(String),
    #[error("tool execution failed: {0:#}")]
    Failed(#[from] anyhow::Error),
    #[error("tool was cancelled")]
    Cancelled,
}
