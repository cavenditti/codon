//! Tools the agent can call mid-turn. A tool advertises a JSON schema
//! to the model, then receives a JSON value the model produced and
//! returns text (or an error) for the model to consume next turn.
//!
//! Tools are the codon-side hook for everything outside the model:
//! file IO, pane reads, shell history search, memory writes, and
//! agent-as-tool delegation (see [`crate::runtime::delegate`]).

use crate::runtime::cancel::CancelToken;
use crate::runtime::error::ToolError;
use anyhow::Result;
use gpui::AsyncApp;
use std::sync::Arc;

/// Single-shot tool invocation. Kept simple — text in, text out;
/// structured outputs round-trip through JSON-in-text by convention.
///
/// `?Send` because tool implementations frequently call back into
/// GPUI via `AsyncApp`, which is `!Send` (the foreground task runs
/// pinned to one thread). The agent runtime always drives tools from
/// the foreground spawn, so Send isn't gained by requiring it here —
/// only false bug reports are.
#[async_trait::async_trait(?Send)]
pub trait Tool: Send + Sync + 'static {
    /// Canonical tool name as advertised to the model. Must be a
    /// stable identifier — the model may key on it across turns.
    fn name(&self) -> &str;

    /// One-paragraph description shown to the model. Should describe
    /// when to call it, not how the implementation works.
    fn description(&self) -> &str;

    /// JSON schema for the `input` argument the model will produce.
    /// Use the conventional Draft 7 shape (`{"type":"object", ...}`).
    fn input_schema(&self) -> serde_json::Value;

    /// The tool input as it is allowed to appear in the metadata-only
    /// trace. Defaults to the raw input; tools whose args carry bytes
    /// that must never be recorded (a shell command, a secret) override
    /// this to substitute a shape-only placeholder so the trace stays
    /// body-free (REQ:codon/agent-routing-harness#c-monitoring).
    fn trace_args(&self, input: &serde_json::Value) -> serde_json::Value {
        input.clone()
    }

    /// Execute the tool. `input` is the parsed JSON the model sent.
    /// The runtime catches panics around this call and surfaces them
    /// as `ToolError::Failed` so the model sees a structured error
    /// instead of the turn dying mid-loop.
    async fn run(
        &self,
        input: serde_json::Value,
        cancel: CancelToken,
        cx: AsyncApp,
    ) -> Result<String, ToolError>;
}

/// Ordered set of tools attached to an agent. Stable iteration order
/// matters because the request envelope sent to the model lists tools
/// in this order, and some providers cache against that sequence.
#[derive(Default, Clone)]
pub struct ToolSet {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, tool: Arc<dyn Tool>) -> Self {
        self.push(tool);
        self
    }

    pub fn push(&mut self, tool: Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Tool>> {
        self.tools.iter()
    }

    pub fn find(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == name)
    }
}
