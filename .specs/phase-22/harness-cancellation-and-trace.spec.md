---
id: TASK:phase-22/harness-cancellation-and-trace
type: task
status: accepted
version: 0.1.0
summary: >
  Plumb the cancellation token through model calls + tool calls; add
  the per-turn trace recorder (capped 50 turns) plus a TraceViewer
  picker. Trace records metadata only — message bodies are never
  recorded.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-harness#c-cancellation
  - REQ:codon/agent-harness#c-trace
  - REQ:codon/agent-harness#c-trace-redaction
aspects: [cancel-token, trace-recorder, trace-redaction]
---

# Harness cancellation + trace recorder

## Plan

- Cancellation:
  - `CancelToken` is a thin wrapper over `tokio_util::sync::Cancellation
    Token` (or the equivalent in GPUI's async runtime).
  - The harness creates one token per `run_turn`. The overlay's Esc
    handler calls `token.cancel()`. The model client respects the
    token and aborts its in-flight HTTP call.
  - Tools poll the token periodically (the `pane-tools-budget` task
    wires this); long loops abort within 50 ms.
  - Workspace close: every active harness turn's token is
    cancelled.
- Trace recorder:
  - New `crates/codon-agent/src/harness/trace.rs` with
    `TurnTrace { id, started: SystemTime, phases:
    Vec<PhaseEvent>, tool_calls: Vec<ToolEvent>, outcome:
    TraceOutcome }`.
  - PhaseEvent shapes: `PreambleBuilt { byte_count }`,
    `ModelCallStarted`, `ModelCallFinished { tokens_in,
    tokens_out }`, `Cancelled`.
  - ToolEvent: `{ name, args_summary, result_shape, latency_ms }`.
    `args_summary` is a one-line printable form (e.g.
    `grep_current_pane{pattern="ERROR", max_hits=10}`); full args
    are NOT recorded.
  - `result_shape` describes the shape only (`ok`, `error:<kind>`,
    `truncated`); result bodies are NOT recorded.
  - Ring buffer: 50 turns per session, in-memory, dropped on
    workspace close.
- TraceViewer:
  - New `codon_agent::TraceViewer` action — `prefix t r` would be a
    natural bind. Confirm `prefix t` isn't already a multi-key
    chord owning that prefix (it's `GotoOrOpenTerminal` — so use
    `prefix shift-r` or similar; pick during PR).
  - Picker over the 50 entries, ordered newest-first. Enter opens
    a read-only buffer with the trace pretty-printed.

## Acceptance

- Esc during an in-flight HTTP model call returns
  `TurnOutcome::Cancelled` within 100 ms (test against a stub
  model client that sleeps).
- Trace for a successful turn contains the documented PhaseEvents
  in order; no message-body fields are present (asserted via
  serde introspection).
- Trace ring buffer holds at most 50 entries; the 51st evicts
  the oldest.
- `cargo test -p codon-agent` passes.
