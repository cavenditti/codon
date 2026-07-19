//! Agent-as-tool delegation. Wrap any [`Agent`] in [`DelegateTool`],
//! drop it into a parent agent's [`ToolSet`], and the parent can hand
//! off subtasks. The child runs to completion (its own tool loop, its
//! own model, its own prompt) and returns its final text as the tool
//! result.
//!
//! This is the foundation for orchestration patterns where a planner
//! agent picks between specialised workers (a code-search agent, a
//! file-edit agent, etc.). The parent doesn't see the child's
//! intermediate turns — only the final answer.

use crate::runtime::agent::Agent;
use crate::runtime::cancel::CancelToken;
use crate::runtime::error::ToolError;
use crate::runtime::registry::AgentRegistry;
use crate::runtime::tool::Tool;
use anyhow::anyhow;
use gpui::AsyncApp;
use std::sync::Arc;

pub struct DelegateTool {
    name: String,
    description: String,
    schema: serde_json::Value,
    /// Registry name of the delegation target. Resolved from the
    /// [`AgentRegistry`] at call time so `[agent.<name>]` overrides
    /// applied after flow-compile still reach the delegated agent; the
    /// pinned `agent` below is the fallback if the name is no longer
    /// registered.
    target_name: Arc<str>,
    agent: Arc<Agent>,
    parent_agent: Option<Arc<str>>,
}

impl DelegateTool {
    /// Construct a delegate exposing `agent` under `name`.
    /// `description` is the tool-blurb shown to the parent model;
    /// say what the child can do and when to call it.
    pub fn new(name: impl Into<String>, description: impl Into<String>, agent: Arc<Agent>) -> Self {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "The task description for the sub-agent."
                }
            },
            "required": ["input"]
        });
        Self {
            name: name.into(),
            description: description.into(),
            schema,
            target_name: agent.name.clone(),
            agent,
            parent_agent: None,
        }
    }

    pub fn with_parent_agent(mut self, parent_agent: Arc<str>) -> Self {
        self.parent_agent = Some(parent_agent);
        self
    }
}

#[async_trait::async_trait(?Send)]
impl Tool for DelegateTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn input_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    async fn run(
        &self,
        input: serde_json::Value,
        cancel: CancelToken,
        cx: AsyncApp,
    ) -> Result<String, ToolError> {
        let user = input
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::BadInput("expected `input: string`".to_string()))?
            .to_string();
        // Resolve the target by name on every call so a user
        // `[agent.<name>]` override applied after flow-compile reaches
        // the delegated agent. Falls back to the pinned Arc only when
        // the target is no longer registered.
        let target = cx
            .update(|app| AgentRegistry::get(app, self.target_name.as_ref()))
            .unwrap_or_else(|| self.agent.clone());
        let outcome = if let Some(parent_agent) = &self.parent_agent {
            target
                .run_as_child(&user, cancel, &cx, parent_agent.clone())
                .await
        } else {
            target.run(&user, cancel, &cx, None).await
        }
        .map_err(|err| ToolError::Failed(anyhow!("sub-agent failed: {err}")))?;
        Ok(outcome.text)
    }
}
