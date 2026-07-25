---
id: REQ:codon/agent-roster
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Routing flows can express a tiered agent roster ergonomically:
  prompts load from files, delegation handoffs carry a typed
  status-block contract with deterministic fail-open enforcement, and
  codon ships a documented example orchestrator/implementer/reviewer
  flow with tiered models.
owners: [carlo]
refines:
  - REQ:codon/agent-routing-harness#c-rhai-declarations
  - REQ:codon/agent-routing-harness#c-delegation
aspects: [prompt-file-vocabulary, delegation-report-contract]
categorized_under: []
---

# Agent roster

## Context

The routing harness declares agents and handoffs in Rhai, but real
rosters — Carlo's opencode setup is the reference
(`~/.config/opencode/agent/*.md`: orchestrator / coder / reviewer /
guru with per-agent models, a structured status-block reporting
protocol, and a fail-open enforcement plugin) — need three ergonomics
the flow API lacks: multi-hundred-line prompts don't belong in Rhai
string literals; a parent agent needs a machine-checkable contract on
what a delegated child reports back; and a new user needs a working
tiered flow to copy, not a syntax reference.

The status-block idea ports strengthened: opencode enforces it with a
best-effort TypeScript hook; codon parses it into a typed struct in the
runtime and folds deterministic `[enforce]` warnings into the tool
result the parent model actually sees.

:::{requirement id="agent-roster" level="MUST"}
- {#c-prompt-files} A flow agent declaration MUST accept
  `prompt_file:` as an alternative to inline `prompt:` — resolved
  relative to the flow file's directory, read at flow-compile time,
  with a missing/unreadable file failing the flow load under the
  existing last-good semantics
  (REQ:codon/agent-routing-harness#c-last-good). Declaring both
  `prompt:` and `prompt_file:` is a compile error.
- {#c-status-block} The runtime MUST define a typed delegation status
  block — status (done | done-with-concerns | blocked), confidence,
  spec_issues, deviations, files, verification, warnings — with a
  canonical text rendering. A handoff opts in via
  `handoff(from, to, #{ report: true, ... })`; the child's trailing
  status block is then parsed into the typed struct and recorded.
- {#c-enforcement} For report-enabled handoffs the runtime MUST run a
  deterministic post-hoc enforcement pass over the child's reply:
  missing markers are detected and appended to the tool result as
  `[enforce]` warning lines plus trace metadata. Enforcement is
  fail-open — it never blocks, truncates, or rewrites the child's
  content beyond appending warnings.
- {#c-example-flow} Codon MUST ship a documented example flow under
  `assets/config/flows/` — an orchestrator / implementer / reviewer /
  safety roster with tiered model assignments (cheap generator,
  stronger verifier), prompt files, report-enabled handoffs, and a
  shell safety chain — referenced from `codon.example.toml`, and the
  example MUST compile in a test so it cannot drift from the flow API.
:::
