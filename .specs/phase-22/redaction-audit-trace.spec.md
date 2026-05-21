---
id: TASK:phase-22/redaction-audit-trace
type: task
status: accepted
version: 0.1.0
summary: >
  Emit a `RedactionEvent` into the harness trace on every
  pipeline call (counts + kinds only, never bodies). Add
  `codon_redact::AuditPicker` so the user can review what's
  being scrubbed.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/secret-redaction-pipeline#c-audit-trace
aspects: [trace-event, audit-picker]
blocked_by:
  - TASK:phase-22/redaction-pipeline-trait
  - TASK:phase-22/harness-cancellation-and-trace
---

# Redaction audit trace + picker

## Plan

- Trace event:
  - Extend `PhaseEvent` (in
    `crates/codon-agent/src/harness/trace.rs`) with a new
    variant:
    ```rust
    PhaseEvent::Redaction(RedactionEvent {
        caller: &'static str,         // "command_history" | "project_kb" | "memory_remember" | "preamble"
        stage_counts: StageCounts,    // hits per stage
        spans_total: usize,
        kinds_seen: Vec<SpanKind>,    // dedup'd
        outcome: TraceOutcome,        // Clean | Redacted | Risky
    })
    ```
  - The pipeline calls `cx.harness_trace.record_redaction(...)`
    on every `redact` exit. Bodies (input or output) are
    forbidden in the struct by construction — no `String` field
    that could be misused. Asserted by a serde-introspection
    test (same pattern as the existing trace-redaction test).
- AuditPicker:
  - New `codon_redact::AuditPicker` modal listing the most
    recent N redaction events from the current session (default
    N = 100, drawn from the trace ring buffer).
  - Rows: `<ts> <caller> <outcome> <kinds>`. Enter opens a
    detail view (read-only) showing the stage counts + kinds.
  - `prefix shift-r` is the proposed bind. Confirm no
    collision; rebind during PR if needed.
- The picker has a "this session only" scope. Cross-session
  redaction audit is out of scope; the trace is in-memory.

## Acceptance

- A redaction event for a `command_history.summarize` flow
  appears in the trace with `caller = "command_history"`,
  outcome and kinds populated, body fields absent.
- Serde-introspection test fails the build if any future code
  change adds a body field to `RedactionEvent`.
- `cmd-k shift-r` opens the AuditPicker.
- Enter on a row opens the detail view.
- `cargo test -p codon-redact` and `cargo test -p codon-agent`
  pass.
