---
id: TASK:phase-23/shell-execute
type: task
status: accepted
version: 0.1.0
summary: >
  Approved shell commands actually execute — sh -c with cwd,
  kill-on-cancel, byte-capped combined output, exit code — and every
  safety decision reaches the trace with its deciding layer.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-shell-safety#c-execution
  - REQ:codon/agent-shell-safety#c-safety-trace
aspects: [execution, safety-trace]
assignee:
eta:
blocked_by: []
---

# Shell execute

## Plan

- Replace the approval-only stub in
  [ShellCommandTool](spec:src:crates/codon-agent/src/runtime/routing.rs)
  with real execution after an `allow` verdict: spawn `/bin/sh -c
  <command>` via `smol::process` (never the user's interactive shell),
  honoring `cwd` when provided; race child completion against the
  `CancelToken` and kill the child on cancellation.
- Combined stdout+stderr, byte-capped (32 KiB) with an explicit
  `[truncated]` marker; the result always carries the exit code.
- Thread the final `SafetyVerdict` into the trace: replace the
  string-sniffing `safety_decision_for_tool_result` in
  [agent.rs](spec:src:crates/codon-agent/src/runtime/agent.rs) with a
  structured decision channel from the tool (decision, source layer,
  risk, escalated flag — never command bytes).
- Update `tests/harness_integration.rs` expectations from
  "safety_approved: command not executed" to real round-trips.

## Acceptance

Integration tests: an allowlisted `echo` round-trips its output and
exit code; a denied command never spawns; cancellation kills a
long-running child promptly; oversized output is capped with the
marker; the trace records decision + source for every shell dispatch.
`cargo test -p codon-agent` green.
