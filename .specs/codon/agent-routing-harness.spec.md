---
id: REQ:codon/agent-routing-harness
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Codon can load a named, user-editable Rhai routing flow that defines
  agents, prompts, model selectors, delegation tools, and shell-command
  safety policy without recompiling Rust.
owners: [carlo]
refines: [REQ:codon/agent-harness#c-no-vendor-lock, REQ:codon/agent-harness#c-tool-dispatch]
aspects: [model-boundary, tool-dispatch]
categorized_under: []
---

# Agent routing harness

## Context

The in-house `codon_agent::runtime` loop already supports model-client
indirection, tool dispatch, agent-as-tool delegation, cancellation, and a
metadata-only trace surface. What is still hardcoded is the topology:
built-in agents are registered from Rust, and delegation patterns require
Rust edits.

Codon needs a faster iteration path for coding harness experiments. Users
should be able to define a flow with a main agent, critique/reckoning
agents, edit appliers, and safety evaluators in a script file, then switch
the active flow from the unified `codon.toml`. Provider access should keep
using Zed's language-model registry, including OpenRouter and local
providers, rather than adding provider-specific HTTP clients to Codon.

:::{requirement id="agent-routing-harness" level="MUST"}
- {#c-active-flow} Codon MUST read `[agent_harness] active_flow` from
  `codon.toml` and resolve it to a Rhai flow file under the configured
  flow paths, defaulting to `~/.config/codon/flows/<name>.rhai`.
- {#c-rhai-declarations} A flow file MUST be able to declare agents with
  model selectors, prompts, temperatures, turn budgets, enabled tools, and
  an entrypoint using a small sandboxed Rhai API.
- {#c-delegation} A flow file MUST be able to expose one scripted agent to
  another as a named delegation tool backed by the existing `DelegateTool`
  runtime.
- {#c-provider-boundary} Scripted models MUST resolve through the existing
  `ModelSpec` / Zed language-model registry boundary. OpenRouter is used by
  selecting provider-qualified model strings; Codon does not add a new
  OpenRouter HTTP client for this requirement.
- {#c-last-good} If a flow file fails to parse or compile, Codon MUST keep
  the last successfully loaded routing registry active and surface a
  metadata-only configuration error in logs/trace state.
- {#c-shell-safety} Harness-initiated shell commands MUST be gated by a
  configured safety agent before execution. If the safety agent is missing
  or unavailable, shell execution fails closed unless the flow explicitly
  opts into a fail-open development mode.
- {#c-monitoring} The trace surface MUST identify the active flow, parent
  agent, delegated child agent, and shell safety decision without recording
  user prompt bodies or full tool outputs.
- {#c-tests} The routing loader, dynamic delegation, last-good reload
  behaviour, and shell-safety gate MUST be covered by no-network tests with
  stub model clients.
:::
