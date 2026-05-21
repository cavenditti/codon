---
id: TASK:phase-22/preamble-project-summary-surface
type: task
status: accepted
version: 0.1.0
summary: >
  Refines REQ:codon/agent-context-preamble's new
  `c-project-summary-surface` clause. Wraps the project-kb
  surface call inside the preamble's existing memory-section
  collector. Same byte-budget rules; same determinism; silently
  omitted when project-kb is disabled.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-context-preamble#c-project-summary-surface
aspects: [preamble-integration]
blocked_by:
  - TASK:phase-22/project-kb-preamble
---

# Preamble: project-summary section

## Plan

- This task is the preamble-side glue for the project-kb
  surfacing landed in
  `TASK:phase-22/project-kb-preamble` (which delivers the
  collector + budget split). This task is the *call site* —
  the preamble assembler invokes the collector and renders the
  bytes into the assembled string.
- Concretely:
  - The preamble assembler's memory section (currently
    consuming `codon_memory::for_preamble`) is extended to call
    `codon_agent::preamble::surface::collect(...)` instead.
    `collect` is the merged accessor from
    `project-kb-preamble` that pulls both memory and project-kb
    rows.
  - The section header is unchanged for memory-only output;
    for project-kb-only output it reads
    `# memories + project-kb (<scope>)`. When both are present
    the entries appear in order: project-kb at the top, then
    memories.
- Off-feature behaviour: when project-kb is disabled the
  collector returns memories only and the section reads
  exactly as before (backwards-compatible — existing snapshot
  tests in `preamble-budget-determinism` continue to pass).

## Acceptance

- A preamble built with an active directory summary surfaces a
  project-kb block above the memories.
- A preamble built with project-kb disabled produces the same
  bytes as before the addition (regression check against the
  determinism property test).
- The byte budget for the combined memory+project-kb section is
  still ≤ 25% of the overall preamble budget.
- `cargo test -p codon-agent` passes.
