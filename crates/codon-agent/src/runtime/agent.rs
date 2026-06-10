//! [`Agent`] is the codon-side aggregate: a model client + (optional)
//! system prompt + (optional) tool set + per-agent knobs (temperature,
//! max turns, cache hints). `run` drives a multi-turn tool loop until
//! the model emits an end-turn stop, the cancellation token fires, or
//! the per-agent turn budget is exhausted.
//!
//! Single-turn flows (`#@` fish completion, a one-shot summarizer)
//! configure the agent with no tools and naturally take one pass
//! through the loop. Multi-turn flows (contextual-suggest, future
//! orchestrators) add tools and pay for as many trips through the
//! model as the conversation requires.

use crate::runtime::cancel::CancelToken;
use crate::runtime::error::{AgentError, ToolError};
use crate::runtime::event::AgentEvent;
use crate::runtime::model::ModelClient;
use crate::runtime::tool::ToolSet;
use crate::runtime::trace::{TraceLog, TraceOutcome, TurnTrace};
use anyhow::Result;
use futures::channel::mpsc;
use futures::{FutureExt as _, StreamExt as _, select_biased};
use gpui::AsyncApp;
use language_model::{
    LanguageModelCompletionEvent, LanguageModelRequest, LanguageModelRequestMessage,
    LanguageModelRequestTool, LanguageModelToolResult, LanguageModelToolResultContent,
    LanguageModelToolUse, MessageContent, Role, StopReason, TokenUsage,
};
use std::sync::Arc;
use std::time::Instant;

/// All knobs that survive across turns. Constructed once, run many.
pub struct Agent {
    pub name: Arc<str>,
    pub model: Arc<dyn ModelClient>,
    pub system_prompt: Option<String>,
    /// Prepended to the user's message when an agent is invoked
    /// through a hand-off flow (the cross-pane verbs). Pure
    /// model-call flows ignore this — the caller passes whatever
    /// they like as the user message.
    pub user_prefix: Option<String>,
    pub temperature: Option<f32>,
    pub tools: ToolSet,
    pub max_turns: usize,
    pub cache_system_prompt: bool,
}

impl Agent {
    pub fn builder(name: impl Into<Arc<str>>, model: Arc<dyn ModelClient>) -> AgentBuilder {
        AgentBuilder {
            name: name.into(),
            model,
            system_prompt: None,
            user_prefix: None,
            temperature: None,
            tools: ToolSet::new(),
            max_turns: 8,
            cache_system_prompt: true,
        }
    }
}

pub struct AgentBuilder {
    name: Arc<str>,
    model: Arc<dyn ModelClient>,
    system_prompt: Option<String>,
    user_prefix: Option<String>,
    temperature: Option<f32>,
    tools: ToolSet,
    max_turns: usize,
    cache_system_prompt: bool,
}

impl AgentBuilder {
    pub fn system_prompt(mut self, text: impl Into<String>) -> Self {
        self.system_prompt = Some(text.into());
        self
    }
    pub fn user_prefix(mut self, text: impl Into<String>) -> Self {
        self.user_prefix = Some(text.into());
        self
    }
    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }
    pub fn tools(mut self, tools: ToolSet) -> Self {
        self.tools = tools;
        self
    }
    pub fn max_turns(mut self, n: usize) -> Self {
        self.max_turns = n.max(1);
        self
    }
    pub fn cache_system_prompt(mut self, on: bool) -> Self {
        self.cache_system_prompt = on;
        self
    }
    pub fn build(self) -> Agent {
        Agent {
            name: self.name,
            model: self.model,
            system_prompt: self.system_prompt,
            user_prefix: self.user_prefix,
            temperature: self.temperature,
            tools: self.tools,
            max_turns: self.max_turns,
            cache_system_prompt: self.cache_system_prompt,
        }
    }
}

/// What `Agent::run` resolves to. Tool plumbing + intermediate text
/// is reported through the event channel; this struct is what the
/// caller waits on.
#[derive(Debug, Default)]
pub struct TurnOutcome {
    /// Concatenated `Text` deltas from the final assistant turn (the
    /// one that ended with `EndTurn`). Tool-call intermediates are
    /// NOT included.
    pub text: String,
    /// `Stop` reason of the final assistant turn, if any. `None` if
    /// the loop terminated for a reason other than a clean stop.
    pub stop: Option<StopReason>,
    /// Sum of usage deltas reported across every turn.
    pub usage: TokenUsage,
    /// Total turns through the model loop (1 for a single-shot
    /// completion, 1 + number-of-tool-rounds for a tool-using flow).
    pub turns: usize,
}

impl Agent {
    /// Run a single user message through the agent's tool loop. The
    /// returned future resolves to the final assistant text when the
    /// model emits `Stop(EndTurn)`. Intermediate text + every tool
    /// call/result is reported on `events`. Every run records a
    /// metadata-only [`TurnTrace`] into the global [`TraceLog`].
    pub async fn run(
        &self,
        user_input: &str,
        cancel: CancelToken,
        cx: &AsyncApp,
        events: Option<mpsc::UnboundedSender<AgentEvent>>,
    ) -> Result<TurnOutcome, AgentError> {
        let mut trace = TurnTrace::begin(self.name.clone(), self.model.id());
        let result = self
            .run_inner(user_input, &cancel, cx, events, &mut trace)
            .await;
        trace.outcome = match &result {
            Ok(outcome) => TraceOutcome::Ok {
                stop: outcome.stop.map(|s| format!("{s:?}")),
                turns: outcome.turns,
            },
            Err(AgentError::Cancelled) => {
                trace.cancelled();
                TraceOutcome::Cancelled
            }
            Err(AgentError::TooManyTurns(limit)) => TraceOutcome::TooManyTurns { limit: *limit },
            // Record the error *kind* only — provider error messages
            // may quote request payloads (#c-trace-redaction).
            Err(err) => TraceOutcome::Error {
                kind: error_kind(err).to_string(),
            },
        };
        cx.update(|app| TraceLog::push(app, trace));
        result
    }

    async fn run_inner(
        &self,
        user_input: &str,
        cancel: &CancelToken,
        cx: &AsyncApp,
        mut events: Option<mpsc::UnboundedSender<AgentEvent>>,
        trace: &mut TurnTrace,
    ) -> Result<TurnOutcome, AgentError> {
        // A turn fired right after launch (the fish `#@` completer is
        // the realistic case) would otherwise race the startup
        // provider-authentication pass and fail with NoModelAvailable.
        crate::runtime::model::wait_for_provider_authentication(cx).await;

        let mut conversation: Vec<LanguageModelRequestMessage> = Vec::new();
        if let Some(sys) = &self.system_prompt {
            conversation.push(LanguageModelRequestMessage {
                role: Role::System,
                content: vec![MessageContent::Text(sys.clone())],
                cache: self.cache_system_prompt,
                reasoning_details: None,
            });
        }
        let mut user_text = String::new();
        if let Some(prefix) = &self.user_prefix {
            user_text.push_str(prefix);
        }
        user_text.push_str(user_input);
        conversation.push(LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text(user_text)],
            cache: false,
            reasoning_details: None,
        });

        let tool_envelopes = build_tool_envelopes(&self.tools);
        let mut outcome = TurnOutcome::default();

        for _turn in 0..self.max_turns {
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            outcome.turns += 1;

            let request = LanguageModelRequest {
                messages: conversation.clone(),
                tools: tool_envelopes.clone(),
                temperature: self.temperature,
                ..Default::default()
            };
            trace.model_call_started(outcome.turns);
            // Race the model call against cancellation so an Esc lands
            // promptly even while the provider is quiet — the token's
            // waker fires the very poll after `cancel()`.
            let mut cancel_fired = std::pin::pin!(cancel.cancelled().fuse());
            let mut connect = std::pin::pin!(self.model.stream_completion(request, cx).fuse());
            let stream = select_biased! {
                _ = cancel_fired.as_mut() => return Err(AgentError::Cancelled),
                stream = connect => stream.map_err(AgentError::from)?,
            };
            let mut stream = stream.fuse();

            let mut assistant_text = String::new();
            let mut pending_calls: Vec<PendingToolCall> = Vec::new();
            let mut stop_reason: Option<StopReason> = None;
            let mut call_usage: Option<TokenUsage> = None;

            loop {
                let next_event = select_biased! {
                    _ = cancel_fired.as_mut() => return Err(AgentError::Cancelled),
                    event = stream.next() => event,
                };
                let Some(event) = next_event else { break };
                let event = event.map_err(AgentError::from)?;
                match event {
                    LanguageModelCompletionEvent::Text(chunk) => {
                        if let Some(tx) = events.as_mut() {
                            let _ = tx.unbounded_send(AgentEvent::Text(chunk.clone()));
                        }
                        assistant_text.push_str(&chunk);
                    }
                    LanguageModelCompletionEvent::ToolUse(call) if call.is_input_complete => {
                        if let Some(tx) = events.as_mut() {
                            let _ = tx.unbounded_send(AgentEvent::ToolCall {
                                name: call.name.clone(),
                                input: call.input.clone(),
                            });
                        }
                        pending_calls.push(PendingToolCall {
                            call,
                            parse_error: None,
                        });
                    }
                    LanguageModelCompletionEvent::ToolUseJsonParseError {
                        id,
                        tool_name,
                        raw_input: _,
                        json_parse_error,
                    } => {
                        // Fail-soft (#c-fail-soft): keep the call in the
                        // conversation so every tool_result has a matching
                        // tool_use, but skip dispatch — the parse error is
                        // fed straight back so the model can retry.
                        pending_calls.push(PendingToolCall {
                            call: LanguageModelToolUse {
                                id,
                                name: tool_name,
                                raw_input: String::new(),
                                // Empty object rather than Null — providers
                                // reject Null inputs on conversation replay.
                                input: serde_json::Value::Object(Default::default()),
                                is_input_complete: true,
                                thought_signature: None,
                            },
                            parse_error: Some(json_parse_error),
                        });
                    }
                    LanguageModelCompletionEvent::UsageUpdate(usage) => {
                        if let Some(tx) = events.as_mut() {
                            let _ = tx.unbounded_send(AgentEvent::Usage(usage));
                        }
                        outcome.usage = outcome.usage + usage;
                        call_usage = Some(call_usage.map_or(usage, |prior| prior + usage));
                    }
                    LanguageModelCompletionEvent::Stop(reason) => {
                        stop_reason = Some(reason);
                    }
                    LanguageModelCompletionEvent::Thinking { .. }
                    | LanguageModelCompletionEvent::RedactedThinking { .. }
                    | LanguageModelCompletionEvent::StartMessage { .. }
                    | LanguageModelCompletionEvent::ReasoningDetails(_)
                    | LanguageModelCompletionEvent::Queued { .. }
                    | LanguageModelCompletionEvent::Started
                    | LanguageModelCompletionEvent::ToolUse(_) => {
                        // partial ToolUse events (is_input_complete=false)
                        // are streamed deltas — we wait for the
                        // completion marker before dispatching.
                    }
                }
            }

            let usage_in = call_usage.map(|u| u.input_tokens + u.cache_read_input_tokens);
            let usage_out = call_usage.map(|u| u.output_tokens);
            trace.model_call_finished(outcome.turns, usage_in, usage_out);

            if pending_calls.is_empty() {
                outcome.text = assistant_text;
                outcome.stop = stop_reason;
                return Ok(outcome);
            }

            // Record the assistant turn (text + tool_use entries) so
            // the model sees its own call on the next request.
            let mut assistant_content = Vec::new();
            if !assistant_text.is_empty() {
                assistant_content.push(MessageContent::Text(assistant_text));
            }
            for pending in &pending_calls {
                assistant_content.push(MessageContent::ToolUse(pending.call.clone()));
            }
            conversation.push(LanguageModelRequestMessage {
                role: Role::Assistant,
                content: assistant_content,
                cache: false,
                reasoning_details: None,
            });

            // Run every tool sequentially and gather results into one
            // user message — that's the shape every provider accepts.
            let mut result_content = Vec::with_capacity(pending_calls.len());
            for PendingToolCall { call, parse_error } in pending_calls {
                let dispatch_started = Instant::now();
                let tool_outcome = match parse_error {
                    Some(parse_error) => Err((
                        "malformed_input",
                        format!("tool input was not valid JSON: {parse_error}"),
                    )),
                    None => self.run_one_tool(&call, cancel, cx).await,
                };
                let latency_ms = dispatch_started.elapsed().as_millis() as u64;
                let (is_error, shape, text) = match tool_outcome {
                    Ok(text) => (false, format!("ok({}B)", text.len()), text),
                    Err((kind, message)) => (true, format!("error:{kind}"), message),
                };
                trace.tool_dispatched(
                    Arc::from(call.name.as_ref() as &str),
                    &call.input,
                    shape,
                    latency_ms,
                );
                if let Some(tx) = events.as_mut() {
                    let _ = tx.unbounded_send(AgentEvent::ToolResult {
                        name: call.name.clone(),
                        is_error,
                        output: text.clone(),
                    });
                }
                result_content.push(MessageContent::ToolResult(LanguageModelToolResult {
                    tool_use_id: call.id,
                    tool_name: call.name,
                    is_error,
                    content: vec![LanguageModelToolResultContent::from(text)],
                    output: None,
                }));
            }
            conversation.push(LanguageModelRequestMessage {
                role: Role::User,
                content: result_content,
                cache: false,
                reasoning_details: None,
            });
        }

        Err(AgentError::TooManyTurns(self.max_turns))
    }

    /// Dispatch one well-formed tool call. `Err` carries a stable
    /// shape-kind (for the trace) plus the message folded back to the
    /// model — bodies never reach the trace.
    async fn run_one_tool(
        &self,
        call: &LanguageModelToolUse,
        cancel: &CancelToken,
        cx: &AsyncApp,
    ) -> Result<String, (&'static str, String)> {
        let Some(tool) = self.tools.find(call.name.as_ref()) else {
            return Err(("unknown_tool", format!("unknown tool `{}`", call.name)));
        };
        // Tool::run owns its own error mapping; we adapt the result
        // into the same (Ok, Err) text channel the model consumes.
        match tool
            .run(call.input.clone(), cancel.clone(), cx.clone())
            .await
        {
            Ok(text) => Ok(text),
            Err(ToolError::Cancelled) => Err(("cancelled", "tool was cancelled".to_string())),
            Err(ToolError::BadInput(msg)) => Err(("bad_input", format!("bad tool input: {msg}"))),
            Err(ToolError::Failed(err)) => Err(("failed", format!("tool failed: {err:#}"))),
        }
    }
}

/// A tool call captured from the model stream. `parse_error` marks
/// calls whose input JSON failed to parse — they stay in the
/// conversation (every tool_result needs a matching tool_use) but are
/// never dispatched.
struct PendingToolCall {
    call: LanguageModelToolUse,
    parse_error: Option<String>,
}

/// Stable kind string for [`TraceOutcome::Error`] — never the message,
/// which may quote provider payloads.
fn error_kind(err: &AgentError) -> &'static str {
    match err {
        AgentError::NoModelAvailable => "no_model_available",
        AgentError::UnknownAgent(_) => "unknown_agent",
        AgentError::Cancelled => "cancelled",
        AgentError::TooManyTurns(_) => "too_many_turns",
        AgentError::Model(_) => "model",
        AgentError::EmptyResponse => "empty_response",
        AgentError::Other(_) => "other",
    }
}

fn build_tool_envelopes(tools: &ToolSet) -> Vec<LanguageModelRequestTool> {
    tools
        .iter()
        .map(|t| LanguageModelRequestTool {
            name: t.name().to_string(),
            description: t.description().to_string(),
            input_schema: t.input_schema(),
            use_input_streaming: false,
        })
        .collect()
}
