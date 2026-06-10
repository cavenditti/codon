//! Integration tests for the agent runtime
//! (TASK:phase-22/harness-tests). A scripted [`StubModel`] drives
//! synthetic turns end-to-end through `Agent::run` — tool dispatch,
//! fail-soft error folding, cancellation, the turn budget, and the
//! metadata-only trace recorder. No network, no real provider.

use codon_agent::{
    Agent, AgentError, CancelToken, ModelClient, Tool, ToolError, ToolSet, TraceLog,
};
use futures::StreamExt as _;
use futures::stream::BoxStream;
use gpui::TestAppContext;
use language_model::{
    LanguageModelCompletionError, LanguageModelCompletionEvent, LanguageModelRequest,
    LanguageModelToolResultContent, LanguageModelToolUse, MessageContent, Role, StopReason,
    TokenUsage,
};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Scripted model: each `stream_completion` call pops the next event
/// script and records the request it was sent.
struct StubModel {
    scripts: Mutex<VecDeque<Vec<LanguageModelCompletionEvent>>>,
    requests: Mutex<Vec<LanguageModelRequest>>,
}

impl StubModel {
    fn new(scripts: Vec<Vec<LanguageModelCompletionEvent>>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    /// Text of every error tool_result in request `ix`'s final message.
    fn error_tool_results(&self, ix: usize) -> Vec<String> {
        let requests = self.requests.lock().unwrap();
        let Some(message) = requests[ix].messages.last() else {
            return Vec::new();
        };
        assert_eq!(message.role, Role::User);
        message
            .content
            .iter()
            .filter_map(|content| match content {
                MessageContent::ToolResult(result) if result.is_error => match &result.content[0] {
                    LanguageModelToolResultContent::Text(text) => Some(text.to_string()),
                    other => panic!("unexpected tool result content: {other:?}"),
                },
                _ => None,
            })
            .collect()
    }
}

impl ModelClient for StubModel {
    fn id(&self) -> Arc<str> {
        Arc::from("stub/model")
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        _cx: &gpui::AsyncApp,
    ) -> futures::future::BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>,
            LanguageModelCompletionError,
        >,
    > {
        self.requests.lock().unwrap().push(request);
        let events = self
            .scripts
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| vec![LanguageModelCompletionEvent::Stop(StopReason::EndTurn)]);
        Box::pin(async move { Ok(futures::stream::iter(events.into_iter().map(Ok)).boxed()) })
    }
}

/// Records every input it is dispatched with, echoes it back.
#[derive(Default)]
struct EchoTool {
    calls: Mutex<Vec<serde_json::Value>>,
}

#[async_trait::async_trait(?Send)]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Echoes its input back."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type": "object"})
    }
    async fn run(
        &self,
        input: serde_json::Value,
        _cancel: CancelToken,
        _cx: gpui::AsyncApp,
    ) -> Result<String, ToolError> {
        self.calls.lock().unwrap().push(input.clone());
        Ok(format!("echoed: {input}"))
    }
}

/// Cancels the shared token from inside its own dispatch — exercises
/// cancellation propagating between a tool call and the next model call.
struct CancelTool {
    token: CancelToken,
}

#[async_trait::async_trait(?Send)]
impl Tool for CancelTool {
    fn name(&self) -> &str {
        "cancel_turn"
    }
    fn description(&self) -> &str {
        "Cancels the turn."
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type": "object"})
    }
    async fn run(
        &self,
        _input: serde_json::Value,
        _cancel: CancelToken,
        _cx: gpui::AsyncApp,
    ) -> Result<String, ToolError> {
        self.token.cancel();
        Ok("cancelling".to_string())
    }
}

fn tool_use(id: &str, name: &str, input: serde_json::Value) -> LanguageModelCompletionEvent {
    LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
        id: id.into(),
        name: name.into(),
        raw_input: input.to_string(),
        input,
        is_input_complete: true,
        thought_signature: None,
    })
}

fn text(chunk: &str) -> LanguageModelCompletionEvent {
    LanguageModelCompletionEvent::Text(chunk.to_string())
}

fn stop(reason: StopReason) -> LanguageModelCompletionEvent {
    LanguageModelCompletionEvent::Stop(reason)
}

#[gpui::test]
async fn single_turn_text_completion(cx: &mut TestAppContext) {
    let model = StubModel::new(vec![vec![
        text("ls "),
        text("-la"),
        stop(StopReason::EndTurn),
    ]]);
    let agent = Agent::builder("test", model.clone()).build();
    let outcome = agent
        .run("list files", CancelToken::new(), &cx.to_async(), None)
        .await
        .expect("turn should succeed");
    assert_eq!(outcome.text, "ls -la");
    assert_eq!(outcome.turns, 1);
    assert_eq!(outcome.stop, Some(StopReason::EndTurn));
    assert_eq!(model.request_count(), 1);
}

#[gpui::test]
async fn tool_round_trip(cx: &mut TestAppContext) {
    let model = StubModel::new(vec![
        vec![
            tool_use("t1", "echo", json!({"pattern": "ERROR"})),
            stop(StopReason::ToolUse),
        ],
        vec![text("done"), stop(StopReason::EndTurn)],
    ]);
    let echo = Arc::new(EchoTool::default());
    let agent = Agent::builder("test", model.clone())
        .tools(ToolSet::new().with(echo.clone()))
        .build();
    let outcome = agent
        .run("grep for errors", CancelToken::new(), &cx.to_async(), None)
        .await
        .expect("turn should succeed");
    assert_eq!(outcome.text, "done");
    assert_eq!(outcome.turns, 2);
    assert_eq!(
        echo.calls.lock().unwrap().as_slice(),
        &[json!({"pattern": "ERROR"})]
    );
    assert_eq!(model.request_count(), 2);
}

#[gpui::test]
async fn unknown_tool_fails_soft(cx: &mut TestAppContext) {
    let model = StubModel::new(vec![
        vec![
            tool_use("t1", "not_a_tool", json!({})),
            stop(StopReason::ToolUse),
        ],
        vec![text("recovered"), stop(StopReason::EndTurn)],
    ]);
    let agent = Agent::builder("test", model.clone()).build();
    let outcome = agent
        .run("call something", CancelToken::new(), &cx.to_async(), None)
        .await
        .expect("fail-soft: the turn itself must not error");
    assert_eq!(outcome.text, "recovered");
    let errors = model.error_tool_results(1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("unknown tool"), "got: {}", errors[0]);
}

#[gpui::test]
async fn malformed_tool_input_fails_soft_without_dispatch(cx: &mut TestAppContext) {
    let model = StubModel::new(vec![
        vec![
            LanguageModelCompletionEvent::ToolUseJsonParseError {
                id: "t1".into(),
                tool_name: "echo".into(),
                raw_input: "{not json".into(),
                json_parse_error: "expected value at line 1".to_string(),
            },
            stop(StopReason::ToolUse),
        ],
        vec![text("retried"), stop(StopReason::EndTurn)],
    ]);
    let echo = Arc::new(EchoTool::default());
    let agent = Agent::builder("test", model.clone())
        .tools(ToolSet::new().with(echo.clone()))
        .build();
    let outcome = agent
        .run("echo something", CancelToken::new(), &cx.to_async(), None)
        .await
        .expect("fail-soft: the turn itself must not error");
    assert_eq!(outcome.text, "retried");
    assert!(
        echo.calls.lock().unwrap().is_empty(),
        "malformed input must never reach the tool"
    );
    let errors = model.error_tool_results(1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("not valid JSON"), "got: {}", errors[0]);
    assert!(
        errors[0].contains("expected value at line 1"),
        "got: {}",
        errors[0]
    );
}

#[gpui::test]
async fn pre_cancelled_token_short_circuits(cx: &mut TestAppContext) {
    let model = StubModel::new(vec![vec![text("never"), stop(StopReason::EndTurn)]]);
    let agent = Agent::builder("test", model.clone()).build();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = agent.run("anything", cancel, &cx.to_async(), None).await;
    assert!(matches!(result, Err(AgentError::Cancelled)));
    assert_eq!(
        model.request_count(),
        0,
        "model must not be called after cancel"
    );
}

#[gpui::test]
async fn cancellation_between_tool_and_next_model_call(cx: &mut TestAppContext) {
    let cancel = CancelToken::new();
    let model = StubModel::new(vec![
        vec![
            tool_use("t1", "cancel_turn", json!({})),
            stop(StopReason::ToolUse),
        ],
        vec![text("never reached"), stop(StopReason::EndTurn)],
    ]);
    let agent = Agent::builder("test", model.clone())
        .tools(ToolSet::new().with(Arc::new(CancelTool {
            token: cancel.clone(),
        })))
        .build();
    let result = agent.run("do it", cancel, &cx.to_async(), None).await;
    assert!(matches!(result, Err(AgentError::Cancelled)));
    assert_eq!(
        model.request_count(),
        1,
        "the second model call must not happen after a tool cancelled the turn"
    );
}

#[gpui::test]
async fn cancellation_aborts_an_idle_in_flight_stream(cx: &mut TestAppContext) {
    /// Yields one Text event, then stays pending forever — the loop
    /// must abort via the cancellation race, not via stream progress.
    struct HangingModel;
    impl ModelClient for HangingModel {
        fn id(&self) -> Arc<str> {
            Arc::from("stub/hanging")
        }
        fn stream_completion(
            &self,
            _request: LanguageModelRequest,
            _cx: &gpui::AsyncApp,
        ) -> futures::future::BoxFuture<
            'static,
            Result<
                BoxStream<
                    'static,
                    Result<LanguageModelCompletionEvent, LanguageModelCompletionError>,
                >,
                LanguageModelCompletionError,
            >,
        > {
            Box::pin(async move {
                let hanging = futures::stream::once(async {
                    Ok(LanguageModelCompletionEvent::Text("partial".to_string()))
                })
                .chain(futures::stream::pending());
                Ok(hanging.boxed())
            })
        }
    }

    let agent = Arc::new(Agent::builder("hang", Arc::new(HangingModel)).build());
    let cancel = CancelToken::new();
    let task = cx.foreground_executor().spawn({
        let agent = agent.clone();
        let cancel = cancel.clone();
        let async_cx = cx.to_async();
        async move { agent.run("hello", cancel, &async_cx, None).await }
    });
    // Let the turn start and park on the never-resolving stream…
    cx.run_until_parked();
    // …then cancel: the token's waker must resolve the race immediately.
    cancel.cancel();
    let result = task.await;
    assert!(matches!(result, Err(AgentError::Cancelled)));
}

#[gpui::test]
async fn max_turns_budget_is_enforced(cx: &mut TestAppContext) {
    let looping_turn = || {
        vec![
            tool_use("t1", "echo", json!({"n": 1})),
            stop(StopReason::ToolUse),
        ]
    };
    let model = StubModel::new(vec![looping_turn(), looping_turn(), looping_turn()]);
    let agent = Agent::builder("test", model.clone())
        .tools(ToolSet::new().with(Arc::new(EchoTool::default())))
        .max_turns(2)
        .build();
    let result = agent
        .run("loop forever", CancelToken::new(), &cx.to_async(), None)
        .await;
    assert!(matches!(result, Err(AgentError::TooManyTurns(2))));
    assert_eq!(model.request_count(), 2);
}

#[gpui::test]
async fn usage_accumulates_into_outcome_and_session_totals(cx: &mut TestAppContext) {
    let model = StubModel::new(vec![vec![
        LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
            input_tokens: 200,
            output_tokens: 80,
            ..Default::default()
        }),
        text("hi"),
        stop(StopReason::EndTurn),
    ]]);
    let agent = Agent::builder("test", model.clone()).build();
    let before = cx.update(|app| TraceLog::token_totals(app));
    let outcome = agent
        .run("count this", CancelToken::new(), &cx.to_async(), None)
        .await
        .expect("turn should succeed");
    assert_eq!(outcome.usage.input_tokens, 200);
    assert_eq!(outcome.usage.output_tokens, 80);
    let after = cx.update(|app| TraceLog::token_totals(app));
    assert_eq!(after.0 - before.0, 200);
    assert_eq!(after.1 - before.1, 80);
}

#[gpui::test]
async fn trace_records_metadata_and_redacts_bodies(cx: &mut TestAppContext) {
    const USER_SECRET: &str = "SECRET_USER_INPUT_a8f3";
    const TOOL_SECRET: &str = "SECRET_TOOL_OUTPUT_c91d";

    /// Tool whose result body must never appear in the trace.
    struct SecretTool;
    #[async_trait::async_trait(?Send)]
    impl Tool for SecretTool {
        fn name(&self) -> &str {
            "leaky"
        }
        fn description(&self) -> &str {
            "Returns a secret."
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }
        async fn run(
            &self,
            _input: serde_json::Value,
            _cancel: CancelToken,
            _cx: gpui::AsyncApp,
        ) -> Result<String, ToolError> {
            Ok(TOOL_SECRET.to_string())
        }
    }

    let model = StubModel::new(vec![
        vec![
            tool_use("t1", "leaky", json!({"q": 1})),
            stop(StopReason::ToolUse),
        ],
        vec![text("answer"), stop(StopReason::EndTurn)],
    ]);
    let agent = Agent::builder("traced", model.clone())
        .tools(ToolSet::new().with(Arc::new(SecretTool)))
        .build();
    agent
        .run(USER_SECRET, CancelToken::new(), &cx.to_async(), None)
        .await
        .expect("turn should succeed");

    let entries = cx.update(|app| TraceLog::entries(app));
    let trace = entries
        .first()
        .expect("the turn must have recorded a trace");
    assert_eq!(trace.agent.as_ref(), "traced");

    let json = trace.pretty();
    // Metadata IS present…
    assert!(json.contains("model_call_started"), "got: {json}");
    assert!(json.contains("model_call_finished"), "got: {json}");
    assert!(json.contains("\"leaky\""), "got: {json}");
    assert!(json.contains("ok("), "got: {json}");
    // …message bodies are NOT (#c-trace-redaction).
    assert!(
        !json.contains(USER_SECRET),
        "trace leaked the user message: {json}"
    );
    assert!(
        !json.contains(TOOL_SECRET),
        "trace leaked a tool result body: {json}"
    );
    assert!(
        !json.contains("answer"),
        "trace leaked assistant text: {json}"
    );

    // Phases are ordered: started(1) → finished(1) → started(2) → finished(2).
    let phase_names: Vec<&str> = trace
        .phases
        .iter()
        .map(|p| match p {
            codon_agent::runtime::PhaseEvent::PreambleBuilt { .. } => "preamble",
            codon_agent::runtime::PhaseEvent::ModelCallStarted { .. } => "started",
            codon_agent::runtime::PhaseEvent::ModelCallFinished { .. } => "finished",
            codon_agent::runtime::PhaseEvent::Cancelled { .. } => "cancelled",
        })
        .collect();
    assert_eq!(
        phase_names,
        vec!["started", "finished", "started", "finished"]
    );
    assert_eq!(trace.tools.len(), 1);
}

#[gpui::test]
async fn trace_ring_buffer_caps_at_fifty(cx: &mut TestAppContext) {
    let runs = codon_agent::runtime::TRACE_TURN_CAP + 5;
    let scripts = (0..runs)
        .map(|_| vec![text("ok"), stop(StopReason::EndTurn)])
        .collect();
    let model = StubModel::new(scripts);
    let agent = Agent::builder("looper", model.clone()).build();
    let before = cx.update(|app| TraceLog::entries(app).len());
    for _ in 0..runs {
        agent
            .run("again", CancelToken::new(), &cx.to_async(), None)
            .await
            .expect("turn should succeed");
    }
    let entries = cx.update(|app| TraceLog::entries(app));
    assert_eq!(entries.len(), codon_agent::runtime::TRACE_TURN_CAP);
    assert!(before <= codon_agent::runtime::TRACE_TURN_CAP);
    // Newest first: the head must be the most recent push.
    assert!(entries[0].id > entries[entries.len() - 1].id);
}
