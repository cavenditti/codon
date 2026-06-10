# ADR 0001 — Agent harness: in-house loop; forge as a design reference only

- **Status:** accepted
- **Date:** 2026-06-10
- **Spec:** `REQ:codon/agent-harness#c-evaluate-forge`, `TASK:phase-22/harness-evaluate-forge`
- **Decision:** Build the thin in-house loop (`codon_agent::runtime`). Do **not**
  adopt [forge](https://github.com/antoinezambelli/forge) as a dependency, in any
  form. Treat forge as a *design reference* for guardrails patterns.

## Context

The harness REQ named two candidate paths: adopt forge as the host harness
library, or build a ~500-LOC in-house loop. The REQ described forge as "an agent
harness in Rust whose loop + tool-registry shape looks aligned with codon's
needs", and gated adoption on a written evaluation.

## Evaluation

**The REQ's premise was factually wrong: forge is a Python framework.** The
repository describes itself as "A Python framework for self-hosted LLM
tool-calling and multi-step agentic workflows" (verified 2026-06-10;
`language: Python` per the GitHub API). There is no Rust crate to depend on, no
`Tool` trait to compose with codon's GPUI-rooted closures, and no path to
linking it into a Cargo workspace short of embedding a Python runtime — a
non-starter for a single-binary GPUI editor.

That finding collapses most of the memo's mandated rubric, recorded here for
completeness:

- **API shape:** N/A for linking. Python `@dataclass` ToolCall / validator
  pipeline; cannot implement codon's `fn(&mut AsyncApp, ...) -> Task<Result<…>>`
  tool surface.
- **Runtime compatibility:** N/A — CPython, not a Rust async runtime. GPUI's
  single-threaded foreground executor cannot host it in-process.
- **Dependency footprint:** would add a Python interpreter + package environment
  to a Rust binary. Rejected on footprint alone even if bridging were palatable.
- **License:** MIT — compatible, irrelevant given the above.
- **Maintenance signal:** healthy — 2k+ stars, active weekly commits
  (latest release v0.7.4, 2026-06-03), 4 open issues. Quality is not the
  problem; language is.
- **Sample wire-up / spike:** the planned `examples/agent-harness-forge/` spike
  is moot — there is nothing to wire a Rust pane-tool through. The in-house
  runtime's stub-model integration tests
  (`crates/codon-agent/tests/harness_integration.rs`) serve the role the spike
  was meant to play: a working turn driver exercising one tool end-to-end with
  no real model calls.

## What we borrow from forge anyway

Forge's *guardrails design* is worth copying even though its code is not.
Specifically (from forge v0.7.4 / ADR-016):

1. **Malformed tool args ride the tool-error channel, not a crash or a bare
   retry.** Codon's loop implements the same shape: a `ToolUseJsonParseError`
   from the stream is kept in the conversation (every `tool_result` needs a
   matching `tool_use`) but never dispatched — the parse error is folded back
   to the model as an `is_error` tool result so it can retry
   (`REQ:codon/agent-harness#c-fail-soft`).
2. **Unknown-tool-name is checked before args validation** (cheap check first;
   no point validating args on a hallucinated tool).
3. **Budgeted error recovery** — forge's `max_tool_errors` maps to codon's
   `max_turns` per-agent budget; a follow-up may split consecutive-tool-error
   budgeting out of the turn budget the way forge does.

## Decision

`codon_agent::runtime` is the harness: `Agent { model, system_prompt, tools }`
+ `Agent::run` (the `Harness::run_turn` of the REQ — the entry-point naming
followed the implementation, the REQ's "implementation choice is internal"
clause covers the rename), `ModelClient` as the no-vendor-lock trait boundary
over vendored Zed's `LanguageModel` registry, `ToolSet`/`Tool` for dispatch,
`CancelToken` for cancellation, `TraceLog` for the metadata-only per-turn
trace, and `DelegateTool` for agent-as-tool composition. Forge stays a
reference we re-read when designing guardrails, never a dependency.
