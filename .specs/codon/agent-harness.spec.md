---
id: REQ:codon/agent-harness
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  One shared agent loop drives every codon agent interaction:
  contextual-suggest, the existing cross-pane verbs, and any future
  flow. The harness owns the preamble assembly call, the tool
  registry, the cancellation signal, and a per-turn trace surface
  for debugging. Evaluation of forge
  (https://github.com/antoinezambelli/forge) as the host library
  precedes any irreversible adoption — a thin in-house loop is the
  fallback if forge does not compose with codon's GPUI-rooted tool
  surface.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Agent harness

## Context

Phase 3 wired the cross-pane verbs directly into
`agent_ui::AgentPanel::seed_explain_with_selection`. That works
because each verb does one thing: drop text into the agent's
message editor and let the user hit Enter. Phase 22 changes the
shape — contextual-suggest is a tool-using flow with cancellation
and a confirm overlay. Bolting that on per-feature would diverge
fast.

The harness REQ is the consolidation pass. One Rust surface owns:

- Preamble assembly (calls
  [REQ:codon/agent-context-preamble](spec:REQ:codon/agent-context-preamble)).
- The tool registry from
  [REQ:codon/agent-pane-tools](spec:REQ:codon/agent-pane-tools)
  plus the memory tools from
  [REQ:codon/agent-shared-memory](spec:REQ:codon/agent-shared-memory).
- The model call (delegated to the upstream agent client; codon
  does not re-implement HTTP).
- Tool dispatch, cancellation, and a per-turn trace.

Two paths are on the table for the harness implementation:

1. **Adopt forge.** `antoinezambelli/forge` is an agent harness in
   Rust whose loop + tool-registry shape looks aligned with
   codon's needs. Adoption hinges on whether forge's tool
   trait composes with codon's GPUI-rooted tools (`fn(&mut
   AsyncApp, ...) -> Task<Result<...>>`). The harness REQ has an
   evaluation clause whose TASK delivers a one-page memo + a
   sample wire-up before any decision.
2. **Build a thin in-house loop.** ~500 LOC: a `Turn` struct, a
   `ToolRegistry`, an async loop that calls the model client,
   dispatches tools, and stops on `suggest_*`. The fallback if
   forge does not compose.

Either way the public surface is the same — a
`codon_agent::Harness::run_turn(preamble, user_msg, tools, cancel)
-> Result<TurnOutcome>` API the contextual-suggest verb and any
future flow call. The implementation choice is internal.

:::{requirement id="agent-harness" level="MUST"}
The harness MUST:

- {#c-shared-api} expose one public entry point —
  `codon_agent::Harness::run_turn` — used by every agent flow in
  codon. The phase-3 cross-pane verbs are migrated to call it as
  part of phase 22's deliverable
- {#c-evaluate-forge} a phase-22 task delivers a written
  evaluation of [forge](https://github.com/antoinezambelli/forge):
  one-page memo covering API shape, GPUI/Tokio runtime
  compatibility, dependency footprint, license, maintenance
  status, and a working sample wire-up that calls one pane-tool.
  The memo lands at `docs/decisions/0001-agent-harness.md` (or
  the next ADR slot) before any irreversible adoption
- {#c-tool-dispatch} the harness holds the tool registry (pane
  tools from REQ:codon/agent-pane-tools + memory tools from
  REQ:codon/agent-shared-memory + the reply-shaping
  `suggest_command` / `suggest_action` / `suggest_response`
  tools). Tools are registered once at workspace init and
  available to every turn
- {#c-router-gate} on every tool call the harness consults the
  pane-router from
  REQ:codon/agent-contextual-suggest#c-pane-router and returns a
  structured tool error to the model when the call is illegal for
  the active pane. Lets the agent retry with a legal shape rather
  than failing the turn
- {#c-cancellation} every turn carries a cancellation token that
  fires on (a) the user pressing Escape while the overlay is open
  and (b) workspace close. Cancellation propagates to the in-flight
  model call and to any active tool. A cancelled turn returns
  `TurnOutcome::Cancelled` and the overlay closes silently
- {#c-trace} the harness records a per-turn trace: timestamp of
  each phase (preamble built, model call started, tool call
  started/finished, suggestion shape returned), every tool call's
  args + result, and the final outcome. The trace is in-memory
  per session, capped at the last 50 turns. A `codon_agent::
  TraceViewer` action opens the picker over the buffer
- {#c-trace-redaction} the trace MUST NOT include the message body
  the agent sent or received — only metadata (tool name, args,
  outcome shape). Inspecting full bodies is an explicit
  follow-up; the default is privacy-by-omission
- {#c-existing-verbs-migrated} `AgentExplain`, `AgentSummarize`,
  `AgentRefactor` are migrated to call the shared harness in the
  same phase. Their selection-seeded prompt flow continues to
  work; what changes is that they get the unconditional preamble
  and the same cancellation surface
- {#c-cost-bookkeeping} the trace records input/output token
  counts per turn when the model client returns them. A status
  bar item (gated behind `[agent_harness] show_token_counter =
  true`) shows the running total for the current session. Default
  off — codon stays terminal-quiet by default
- {#c-no-vendor-lock} the harness MUST NOT directly reference any
  one model provider. The model client is the vendored
  agent crate's existing surface; the harness consumes it through
  a trait so a future provider swap stays scoped
- {#c-fail-soft} a malformed tool call (the model returns an
  unregistered tool name, or args that don't match the schema)
  returns a structured tool error to the model. The harness does
  not panic and does not surface a toast — the model gets the
  signal and retries
- {#c-tests} the harness has integration tests with a stub model
  client that drives synthetic turns (`expect_tool("grep_current
  _pane")` style). Pane tools are tested through these synthetic
  flows; cancellation has a dedicated test
:::

## Out of scope

- Multi-agent orchestration. Phase 22 ships one agent per turn.
- Streaming UI for in-progress turns. The overlay shows a spinner
  + cancel hint; streaming partial token output into the overlay
  is deferred.
- Per-tool permission prompts. The reply-shaping tools already
  funnel through the confirm-overlay; inspection tools are
  read-only by design, so a permission layer would be ceremony
  without benefit.
- A vendored copy of forge. Adoption — if the evaluation
  recommends it — uses forge as a regular crates.io dependency.
  Vendoring is only on the table if a fix needs to land upstream
  faster than upstream can ship.
