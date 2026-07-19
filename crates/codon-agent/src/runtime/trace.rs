//! Per-turn trace recorder (REQ:codon/agent-harness#c-trace).
//!
//! Every `Agent::run` records one [`TurnTrace`] into the process-wide
//! [`TraceLog`] ring buffer (capped at [`TRACE_TURN_CAP`] turns,
//! in-memory only, gone on exit). The trace is **metadata only** —
//! message bodies, tool result bodies, and full tool args are never
//! recorded (#c-trace-redaction). `args_summary` is a one-line,
//! truncated printable form; `result_shape` describes the outcome
//! shape (`ok(123B)`, `error:bad_input`), never the content.
//!
//! The log also owns the per-session token accumulator
//! (#c-cost-bookkeeping): every pushed turn's usage is saturating-added
//! into running input/output totals that the status-bar counter reads.

use gpui::{App, BorrowAppContext as _, Global};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Ring-buffer capacity of [`TraceLog`]. The 51st turn evicts the oldest.
pub const TRACE_TURN_CAP: usize = 50;

/// Maximum printable length of a tool-call `args_summary`. Longer
/// inputs are truncated with an ellipsis — the trace is a debugging
/// aid, not a transcript.
const ARGS_SUMMARY_MAX: usize = 120;

/// Timestamped phase transitions inside one turn. `at_ms` is the
/// offset from the turn's start (monotonic clock).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum PhaseEvent {
    /// Reserved for the preamble assembler (phase-22 sibling task);
    /// recorded once REQ:codon/agent-context-preamble lands.
    PreambleBuilt {
        at_ms: u64,
        byte_count: usize,
    },
    ModelCallStarted {
        at_ms: u64,
        /// 1-based trip through the tool loop.
        turn: usize,
    },
    ModelCallFinished {
        at_ms: u64,
        turn: usize,
        /// `None` when the provider did not report usage.
        tokens_in: Option<u64>,
        tokens_out: Option<u64>,
    },
    Cancelled {
        at_ms: u64,
    },
}

/// One tool dispatch. `args_summary` is truncated and printable;
/// `result_shape` names the outcome shape only.
#[derive(Debug, Clone, Serialize)]
pub struct ToolEvent {
    pub name: Arc<str>,
    pub args_summary: String,
    pub result_shape: String,
    pub safety_decision: Option<String>,
    pub latency_ms: u64,
}

/// How the turn ended. `Error` records the error *kind* only — never
/// the message, which may quote provider payloads.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum TraceOutcome {
    /// The turn is still running (only visible if the viewer opens
    /// mid-turn — traces are pushed on completion, so normally never).
    InFlight,
    Ok {
        stop: Option<String>,
        turns: usize,
    },
    Cancelled,
    TooManyTurns {
        limit: usize,
    },
    Error {
        kind: String,
    },
}

/// Metadata-only record of one `Agent::run` invocation.
#[derive(Debug, Clone, Serialize)]
pub struct TurnTrace {
    /// Assigned by [`TraceLog::push`]; 0 until pushed.
    pub id: u64,
    pub agent: Arc<str>,
    pub flow: Option<Arc<str>>,
    pub parent_agent: Option<Arc<str>>,
    pub model: Arc<str>,
    pub started_unix_ms: u64,
    pub phases: Vec<PhaseEvent>,
    pub tools: Vec<ToolEvent>,
    pub outcome: TraceOutcome,
    pub tokens_in: u64,
    pub tokens_out: u64,
    #[serde(skip)]
    t0: Instant,
}

impl TurnTrace {
    pub fn begin(
        agent: Arc<str>,
        model: Arc<str>,
        flow: Option<Arc<str>>,
        parent_agent: Option<Arc<str>>,
    ) -> Self {
        Self {
            id: 0,
            agent,
            flow,
            parent_agent,
            model,
            started_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            phases: Vec::new(),
            tools: Vec::new(),
            outcome: TraceOutcome::InFlight,
            tokens_in: 0,
            tokens_out: 0,
            t0: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.t0.elapsed().as_millis() as u64
    }

    pub fn model_call_started(&mut self, turn: usize) {
        let at_ms = self.elapsed_ms();
        self.phases
            .push(PhaseEvent::ModelCallStarted { at_ms, turn });
    }

    pub fn model_call_finished(
        &mut self,
        turn: usize,
        tokens_in: Option<u64>,
        tokens_out: Option<u64>,
    ) {
        let at_ms = self.elapsed_ms();
        self.tokens_in = self.tokens_in.saturating_add(tokens_in.unwrap_or(0));
        self.tokens_out = self.tokens_out.saturating_add(tokens_out.unwrap_or(0));
        self.phases.push(PhaseEvent::ModelCallFinished {
            at_ms,
            turn,
            tokens_in,
            tokens_out,
        });
    }

    pub fn cancelled(&mut self) {
        let at_ms = self.elapsed_ms();
        self.phases.push(PhaseEvent::Cancelled { at_ms });
    }

    pub fn tool_dispatched(
        &mut self,
        name: Arc<str>,
        input: &serde_json::Value,
        result_shape: String,
        safety_decision: Option<String>,
        latency_ms: u64,
    ) {
        self.tools.push(ToolEvent {
            name,
            args_summary: summarize_args(input),
            result_shape,
            safety_decision,
            latency_ms,
        });
    }

    /// Pretty-printed JSON for the viewer / clipboard yank. Safe to
    /// surface verbatim — the struct never holds message bodies.
    pub fn pretty(&self) -> String {
        serde_json::to_string_pretty(self)
            .unwrap_or_else(|err| format!("<trace serialization failed: {err}>"))
    }
}

/// One-line printable summary of the model-produced tool args,
/// truncated to [`ARGS_SUMMARY_MAX`] characters. Full args are not
/// recorded by design.
fn summarize_args(input: &serde_json::Value) -> String {
    let compact = serde_json::to_string(input).unwrap_or_else(|_| "<unprintable>".to_string());
    if compact.chars().count() <= ARGS_SUMMARY_MAX {
        return compact;
    }
    let truncated: String = compact.chars().take(ARGS_SUMMARY_MAX).collect();
    format!("{truncated}…")
}

/// Process-wide trace ring buffer + per-session token accumulator.
#[derive(Default)]
pub struct TraceLog {
    /// Newest at the back.
    turns: VecDeque<TurnTrace>,
    next_id: u64,
    total_tokens_in: u64,
    total_tokens_out: u64,
}

impl Global for TraceLog {}

impl TraceLog {
    pub fn install(cx: &mut App) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
    }

    /// Append a finished turn, assigning its id and folding its token
    /// usage into the session totals. Evicts the oldest entry past
    /// [`TRACE_TURN_CAP`]. Goes through `update_global` so
    /// `observe_global::<TraceLog>` subscribers (the status-bar
    /// counter) re-render.
    pub fn push(cx: &mut App, mut trace: TurnTrace) -> u64 {
        Self::install(cx);
        cx.update_global::<Self, _>(|log, _| {
            log.next_id += 1;
            trace.id = log.next_id;
            log.total_tokens_in = log.total_tokens_in.saturating_add(trace.tokens_in);
            log.total_tokens_out = log.total_tokens_out.saturating_add(trace.tokens_out);
            let id = trace.id;
            log.turns.push_back(trace);
            while log.turns.len() > TRACE_TURN_CAP {
                log.turns.pop_front();
            }
            id
        })
    }

    /// Snapshot of the recorded turns, newest first.
    pub fn entries(cx: &App) -> Vec<TurnTrace> {
        if !cx.has_global::<Self>() {
            return Vec::new();
        }
        cx.global::<Self>().turns.iter().rev().cloned().collect()
    }

    /// Running (input, output) token totals for the session.
    pub fn token_totals(cx: &App) -> (u64, u64) {
        if !cx.has_global::<Self>() {
            return (0, 0);
        }
        let log = cx.global::<Self>();
        (log.total_tokens_in, log.total_tokens_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_summary_truncates_long_input() {
        let long = serde_json::json!({ "pattern": "x".repeat(500) });
        let summary = summarize_args(&long);
        assert!(summary.chars().count() <= ARGS_SUMMARY_MAX + 1);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn args_summary_keeps_short_input_verbatim() {
        let short = serde_json::json!({ "pattern": "ERROR" });
        assert_eq!(summarize_args(&short), r#"{"pattern":"ERROR"}"#);
    }

    #[test]
    fn trace_serialization_omits_monotonic_clock() {
        let trace = TurnTrace::begin("a".into(), "m".into(), None, None);
        let json = trace.pretty();
        assert!(!json.contains("t0"));
    }
}
