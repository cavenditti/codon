---
id: TASK:phase-22/harness-rhai-routing
type: task
status: accepted
version: 0.1.0
summary: >
  Load a selected Rhai routing flow from codon.toml, register scripted
  agents and delegation tools dynamically, and gate scripted shell tools
  through a configured safety agent.
owners: [carlo]
progress: in-progress
refines: [REQ:codon/agent-routing-harness#c-active-flow, REQ:codon/agent-routing-harness#c-rhai-declarations, REQ:codon/agent-routing-harness#c-delegation, REQ:codon/agent-routing-harness#c-provider-boundary, REQ:codon/agent-routing-harness#c-last-good, REQ:codon/agent-routing-harness#c-shell-safety, REQ:codon/agent-routing-harness#c-monitoring, REQ:codon/agent-routing-harness#c-tests]
aspects: [active-flow, rhai-api, delegation, provider-boundary, last-good, shell-safety, trace, tests]
assignee:
eta:
blocked_by: []
---

# Harness Rhai routing

## Plan

- Extend `codon_agent::runtime::config` to parse `[agent_harness]`
  `active_flow`, optional `flow_paths`, and shell-safety defaults from
  the unified `codon.toml`.
- Add a routing-flow loader under `crates/codon-agent/src/runtime/` that
  evaluates a restricted Rhai declaration API: `agent`, `handoff`,
  `entrypoint`, and `safety_for`.
- Register scripted agents into `AgentRegistry` after built-ins reset and
  before regular `[agent.<name>]` overrides apply, so user overrides can
  still change models/prompts on scripted agents.
- Build delegation tools from script `handoff` declarations using the
  existing `DelegateTool`; reject unknown source/target agents with a
  structured loader error.
- Add a minimal shell-command tool shape whose dispatch first invokes the
  configured safety agent and fails closed when safety is unavailable or
  denies the command. The first implementation may expose the gate and
  trace path without wiring general-purpose shell execution into UI flows.
- Extend trace metadata with optional `flow`, `parent_agent`, and
  `safety_decision` fields while preserving body redaction.
- Document a default Rhai flow and corresponding `codon.toml` keys in
  `assets/config/codon.example.toml`.

## Acceptance

- `codon.toml` with `[agent_harness] active_flow = "default"` loads
  `flows/default.rhai` and registers script-defined agents live.
- A flow can define `main`, `reckoning`, `edit_applier`, and `safety`
  agents with OpenRouter/local model strings and expose `reckoning` /
  `edit_applier` as delegate tools on `main`.
- Invalid Rhai keeps the previous routing registry active and does not
  remove built-in agents.
- Shell-command tool dispatch consults the configured safety agent and
  denies execution when safety returns a denial or cannot run.
- No prompt bodies, full shell commands, or full tool outputs are added to
  trace entries.
- `cargo test -p codon-agent` and `spec lint` pass.
