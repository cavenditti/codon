//! `agent.complete` RPC handler. Takes `{partial, description, cwd,
//! shell}` and returns `{command_b64}` — a single fish-syntax command
//! line, base64-encoded so the fish-side parser pulls it out with a
//! trivial regex (no escape-handling).
//!
//! Implementation is a thin adapter: shape the request payload into
//! the user-message string the model expects, call the registered
//! `fish_complete` agent through `codon_agent::Agent::run`, then
//! sanitize the response. Model selection, system prompt, temperature,
//! and cache hints all live in the agent's registry entry — every
//! `[agent.fish_complete]` knob in codon.toml takes effect here
//! automatically.
//!
//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary.

use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use codon_agent::{AgentRegistry, CancelToken};
use codon_agent::agents::FISH_COMPLETE;
use gpui::AsyncApp;
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

pub async fn handle(params_json: serde_json::Value, cx: AsyncApp) -> Result<serde_json::Value> {
    let params: Params =
        serde_json::from_value(params_json).context("parse agent.complete params")?;
    if params.partial.trim().is_empty() && params.description.trim().is_empty() {
        return Err(anyhow!("empty request: both `partial` and `description` are blank"));
    }
    let agent = cx
        .update(|app| AgentRegistry::get(app, FISH_COMPLETE))
        .ok_or_else(|| anyhow!("`fish_complete` agent is not registered"))?;
    let user_message = format_user_message(&params);
    let outcome = agent
        .run(&user_message, CancelToken::new(), &cx, None)
        .await
        .map_err(|err| anyhow!("fish_complete agent error: {err}"))?;
    let command = sanitize(&outcome.text);
    if command.is_empty() {
        return Err(anyhow!("model produced an empty response"));
    }
    let command_b64 = BASE64.encode(command.as_bytes());
    Ok(serde_json::json!({ "command_b64": command_b64 }))
}

/// Build the user message the `fish_complete` agent's system prompt is
/// tuned against: a small set of `key: value` lines followed by a
/// trailing instruction. Stable layout so the system-prompt cache
/// stays warm across calls.
fn format_user_message(params: &Params) -> String {
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
    user
}

/// Strip code fences and surrounding whitespace if the model wrapped
/// the command in markdown despite the system prompt. Collapses
/// internal newlines to spaces — we want a single command line.
fn sanitize(text: &str) -> String {
    let mut t = text.trim();
    if let Some(rest) = t.strip_prefix("```") {
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

    #[test]
    fn format_user_message_emits_stable_layout() {
        let params = Params {
            partial: "git lo".to_string(),
            description: "show last 5 commits".to_string(),
            cwd: Some("/tmp".to_string()),
            shell: Some("fish".to_string()),
        };
        let formatted = format_user_message(&params);
        assert!(formatted.starts_with("cwd: /tmp\nshell: fish\n"));
        assert!(formatted.contains("partial: git lo\n"));
        assert!(formatted.contains("description: show last 5 commits\n"));
        assert!(formatted.ends_with("Reply with the complete command only."));
    }
}
