---
id: TASK:phase-23/shell-gate-verdict
type: task
status: accepted
version: 0.1.0
summary: >
  Structured JSON safety verdicts ({decision, risk, categories,
  reason}) with lenient parsing, tool-side contract prompt, optional
  model-stated intent, and fail-safe `ask` — replacing the ALLOW/DENY
  line protocol.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-shell-safety#c-structured-verdict
  - REQ:codon/agent-shell-safety#c-intent
aspects: [verdict-contract, intent-evidence]
assignee:
eta:
blocked_by: []
---

# Shell gate verdict

## Plan

In `runtime/safety.rs` + `runtime/routing.rs`:

- `SafetyVerdict { decision: Allow|Ask|Deny, risk: u8, categories,
  reason, source }` types with serde.
- Lenient reply parsing ported from the reference plugin: strip code
  fences, extract first `{` … last `}`, `serde_json` parse, shape
  validation, clamp risk. Invalid → fail-safe `ask` verdict with
  `invalid-classifier-response` category.
- The classification contract prompt moves tool-side (built in Rust,
  includes command, cwd, optional intent; instructs strict JSON and the
  category taxonomy) so a flow-authored safety-agent prompt can tune
  tone but not weaken the contract.
- `ShellCommandTool` input schema gains an optional `description`
  (intent) field, threaded into the contract prompt as weak untrusted
  evidence; presence-only in the trace.
- Classifier unavailable / erroring resolves to `ask` (which, until
  TASK:phase-23/shell-ask-overlay lands, fails closed to a refusal
  naming that task; `shell_safety_fail_open = true` collapses `ask` to
  allow per REQ:codon/agent-shell-safety#c-ask-decision).

## Acceptance

Parse tests cover fenced/prefixed/bare/invalid JSON and out-of-range
risk; a stub classifier returning prose (no JSON) yields `ask`; the
contract prompt contains command, cwd, and intent when provided.
`cargo test -p codon-agent` green.
