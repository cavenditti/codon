---
id: TASK:phase-22/harness-tests
type: task
status: accepted
version: 0.1.0
summary: >
  Integration tests with a stub `ModelClient` that drives synthetic
  turns end-to-end through the harness, pane tools, and reply-shaping
  paths. Cancellation has a dedicated test.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-harness#c-tests
---

# Harness integration tests

## Plan

- New test module `crates/codon-agent/tests/harness_integration.rs`.
- A stub `ModelClient` with a scripted-response API:
  ```rust
  let model = StubModel::new()
      .expect_tool_call("grep_current_pane", json!({"pattern": "ERROR"}))
      .respond_with_tool_result(/* hits */)
      .expect_tool_call("suggest_response", json!({"text": "Found 2"}))
      .finish();
  ```
- Tests:
  - **Happy path / read tool:** stub `PaneInspect` impl returning
    canned hits; assert the harness dispatches and the final
    outcome is `SuggestResponse { text: "Found 2" }`.
  - **Router gate:** in an editor pane, stub model issues
    `suggest_command` → harness returns `shape_illegal_for_pane`
    tool error → stub retries `suggest_action` → success.
  - **Cancellation:** spawn a turn whose stub model sleeps 1 s;
    cancel after 50 ms; assert `TurnOutcome::Cancelled` returned
    within 100 ms.
  - **Budget exhaustion:** small `turn_byte_budget`; stub issues
    many reads; harness returns `turn_budget_exhausted` after the
    cap.
  - **Trace shape:** after a successful turn, assert the trace has
    PhaseEvents in order, ToolEvents present, no message body
    fields (use serde introspection to enforce).
- Tests use a minimal GPUI test harness (the same one the other
  codon crates use — see `codon-session`'s tests for the pattern).

## Acceptance

- All listed tests pass.
- `cargo test -p codon-agent --test harness_integration` is green.
- The test suite runs in < 5 s locally (no real network).
- `vendor/zed/script/clippy` clean.
