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
use crate::runtime::tool::Tool;
use anyhow::anyhow;
use gpui::AsyncApp;
use std::sync::Arc;

pub struct DelegateTool {
    name: String,
    description: String,
    schema: serde_json::Value,
    agent: Arc<Agent>,
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
            agent,
        }
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
        let outcome = self
            .agent
            .run(&user, cancel, &cx, None)
            .await
            .map_err(|err| ToolError::Failed(anyhow!("sub-agent failed: {err}")))?;
        Ok(outcome.text)
    }
}
