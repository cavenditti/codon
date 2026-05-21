---
id: TASK:phase-22/project-kb-preamble
type: task
status: accepted
version: 0.1.0
summary: >
  Wire the project/directory summary into the preamble's memory
  section. Most-specific match wins (directory beats project);
  falls back to the project summary at workspace root; silently
  omitted when project-kb is disabled.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/project-knowledge-base#c-preamble-surface
aspects: [preamble-section]
blocked_by:
  - TASK:phase-22/project-kb-aggregator
  - TASK:phase-22/preamble-selection-and-memories
---

# Project KB preamble surfacing

## Plan

- Extend `codon_memory::for_preamble` (added in
  `memory-preamble-surface`) into a more general
  `codon_agent::preamble::surface::collect(focused_pane, budget)`
  that pulls from both memory and project-kb stores.
- Selection algorithm for project-kb:
  1. Resolve the focused pane's `cwd` (terminals + FM have one
     directly; editors use the parent of the open file).
  2. Look up the most-specific `kb_summary` whose `path` is a
     prefix of the cwd (`kb_summaries.path` is absolute).
  3. If no directory summary matches, fall back to the project
     summary at the workspace root.
  4. If project-kb is disabled or no row exists, omit the
     section.
- The project-kb summary occupies the same 25%-of-budget cap as
  memories (REQ:codon/agent-context-preamble#c-memories-budgeted).
  When both are competing, project-kb takes the top half of the
  cap and memories the bottom half — keeps both visible.
- Render: a single-line header `project-kb: <relative path>`
  followed by the bounded paragraph body. Section header drops
  if empty (consistent with the assembler's empty-section rule).

## Acceptance

- Preamble built from a terminal at `<workspace_root>/src/auth/`
  surfaces the directory summary for `src/auth/` when one
  exists.
- Same terminal with no directory summary falls back to the
  project summary.
- With project-kb disabled, the section is absent.
- Budget split: when both memories and project-kb compete and
  the cap can fit both, both render.
- Determinism: same inputs → byte-identical preamble (property
  test extends the existing one from
  `preamble-budget-determinism`).
- `cargo test -p codon-agent` passes.
