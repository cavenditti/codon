//! `agent.complete` RPC handler. Takes `{partial, description}` and
//! returns `{command}` — a single fish-syntax command line. MVP
//! shape: no tool calls, no context injection. Sibling phase-22
//! tasks layer those on top.

use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures::StreamExt as _;
use gpui::AsyncApp;
use language_model::{
    LanguageModelRegistry, LanguageModelRequest, LanguageModelRequestMessage, MessageContent, Role,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Params {
    #[serde(default)]
    partial: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    shell: Option<String>,
}

const SYSTEM_PROMPT: &str = "\
You translate a short natural-language description into a single \
fish-shell command line. Output ONLY the command — no fences, no \
prose, no leading prompt, no trailing newline. Use fish syntax \
(parentheses for command substitution, `set` for variables, no \
`$((...))` arithmetic). If a partial command is provided, extend it \
in place; do not repeat it.";

pub async fn handle(params_json: serde_json::Value, cx: AsyncApp) -> Result<serde_json::Value> {
    let params: Params =
        serde_json::from_value(params_json).context("parse agent.complete params")?;
    if params.partial.trim().is_empty() && params.description.trim().is_empty() {
        return Err(anyhow!("empty request: both `partial` and `description` are blank"));
    }
    let model = cx.update(|app| {
        LanguageModelRegistry::read_global(app)
            .default_model()
            .map(|configured| configured.model)
    });
    let Some(model) = model else {
        return Err(anyhow!(
            "no default language model configured — open the agent panel and pick one"
        ));
    };
    let request = build_request(&params);
    let stream = model
        .stream_completion_text(request, &cx)
        .await
        .map_err(|err| anyhow!("language model error: {err}"))?;
    let mut chunks = stream.stream;
    let mut buf = String::new();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|err| anyhow!("language model stream error: {err}"))?;
        buf.push_str(&chunk);
    }
    let command = sanitize(&buf);
    if command.is_empty() {
        return Err(anyhow!("model produced an empty response"));
    }
    // Return the command base64-encoded so the fish-side parser can
    // pluck it from the JSON response with a trivial regex (no
    // backslash-escape handling). The plugin pipes it through
    // `base64 -d` to recover the raw bytes.
    let command_b64 = BASE64.encode(command.as_bytes());
    Ok(serde_json::json!({ "command_b64": command_b64 }))
}

fn build_request(params: &Params) -> LanguageModelRequest {
    let mut user = String::with_capacity(256);
    if let Some(cwd) = &params.cwd {
        user.push_str("cwd: ");
        user.push_str(cwd);
        user.push('\n');
    }
    if let Some(shell) = &params.shell {
        user.push_str("shell: ");
        user.push_str(shell);
        user.push('\n');
    }
    if !params.partial.trim().is_empty() {
        user.push_str("partial: ");
        user.push_str(params.partial.trim());
        user.push('\n');
    }
    if !params.description.trim().is_empty() {
        user.push_str("description: ");
        user.push_str(params.description.trim());
        user.push('\n');
    }
    user.push_str("\nReply with the complete command only.");

    LanguageModelRequest {
        messages: vec![
            LanguageModelRequestMessage {
                role: Role::System,
                content: vec![MessageContent::Text(SYSTEM_PROMPT.to_string())],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text(user)],
                cache: false,
                reasoning_details: None,
            },
        ],
        ..Default::default()
    }
}

/// Strip code fences and surrounding whitespace if the model wrapped
/// the command in markdown despite the system prompt. Collapses
/// internal newlines to spaces — we want a single command line.
fn sanitize(text: &str) -> String {
    let mut t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Drop the opening fence + optional language tag, then strip
        // a closing fence on the trailing side.
        let after_lang = rest.split_once('\n').map(|(_, rest)| rest).unwrap_or(rest);
        t = after_lang;
        if let Some(rest) = t.rsplit_once("```") {
            t = rest.0;
        }
    }
    t.trim()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_code_fences() {
        assert_eq!(sanitize("```fish\nls -la\n```"), "ls -la");
        assert_eq!(sanitize("```\nls -la\n```"), "ls -la");
    }

    #[test]
    fn sanitize_joins_continuation_lines() {
        assert_eq!(
            sanitize("git log --oneline\n  --max-count=5"),
            "git log --oneline --max-count=5"
        );
    }

    #[test]
    fn sanitize_trims_whitespace() {
        assert_eq!(sanitize("\n   ls -la   \n"), "ls -la");
    }
}
