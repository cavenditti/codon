---
id: TASK:phase-22/harness-api
type: task
status: accepted
version: 0.1.0
summary: >
  Implement `codon_agent::Harness::run_turn(preamble, user_msg,
  tools, cancel)` plus the shared tool registry, router-gated tool
  dispatch, fail-soft error shapes, and the model-client trait
  boundary. Picks up the recommendation from
  `harness-evaluate-forge`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-harness#c-shared-api
  - REQ:codon/agent-harness#c-tool-dispatch
  - REQ:codon/agent-harness#c-router-gate
  - REQ:codon/agent-harness#c-no-vendor-lock
  - REQ:codon/agent-harness#c-fail-soft
aspects: [public-api, tool-registry, router-gate, model-client-trait, fail-soft]
blocked_by:
  - TASK:phase-22/harness-evaluate-forge
---

# Harness API + tool dispatch + router gate

## Plan

- New module `crates/codon-agent/src/harness/mod.rs`.
- Public surface:
  ```rust
  pub struct Harness { /* ... */ }
  impl Harness {
      pub fn new(model: Box<dyn ModelClient>, tools: ToolRegistry) -> Self;
      pub async fn run_turn(
          &self,
          preamble: String,
          user_msg: String,
          cancel: CancelToken,
          cx: &mut AsyncApp,
      ) -> Result<TurnOutcome, HarnessError>;
  }
  pub enum TurnOutcome {
      Suggestion(SuggestionShape),
      Cancelled,
  }
  ```
- `ToolRegistry` holds the pane tools (from pane-tools tasks) +
  memory tools (from memory-tools task) + the three reply-shaping
  tools. Registered at workspace init via a single
  `codon_agent::register_tools(workspace, cx)` entry.
- Router gate: before dispatching any tool, call
  `pane_router::resolve(...)` and check the tool's reply-shape (if
  any) is in `allowed_shapes`. Mismatch → return the structured
  tool error to the model. *Read tools are never gated* — only the
  three `suggest_*` shapes are pane-routed.
- Model client boundary: `pub trait ModelClient { fn complete(...)
  -> BoxFuture<...>; }`. Phase 22 implements one impl that wraps
  the upstream agent crate's client; future providers swap by
  swapping the trait impl. No model-specific code in `harness/`
  outside the trait surface.
- Fail-soft on malformed tool calls:
  - Unregistered tool name → return tool-error to model.
  - Args schema mismatch → return tool-error.
  - Tool panicked → catch via `catch_unwind` (where feasible),
    convert to tool-error. Do not propagate panics to the GPUI
    main loop.
- The harness loop terminates on:
  - The model returns a `suggest_*` shape (success).
  - Cancellation fires.
  - Hard turn budget exceeded (sibling task `pane-tools-budget`).
  - 8 consecutive tool errors without progress (configurable,
    `[agent_harness] max_consecutive_errors = 8`).

## Acceptance

- Synthetic test: stub `ModelClient` issues one tool call →
  harness dispatches → model returns `suggest_response` →
  `TurnOutcome::Suggestion(...)` returned.
- Synthetic test: model issues `suggest_command` from an editor
  pane → router rejects → tool error surfaced → model retries
  with `suggest_action` → success.
- Tool name not in registry → tool error to model; no panic.
- `cargo test -p codon-agent` passes.
