---
id: TASK:phase-23/roster-status-blocks
type: task
status: accepted
version: 0.1.0
summary: >
  Typed delegation status blocks (status/confidence/spec issues/
  deviations/files/verification/warnings) parsed from report-enabled
  handoffs, with deterministic fail-open [enforce] warnings.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-roster#c-status-block
  - REQ:codon/agent-roster#c-enforcement
aspects: [status-block, enforcement]
assignee:
eta:
blocked_by: []
---

# Roster status blocks

## Plan

- `runtime/report.rs`: `StatusBlock` struct (status: done |
  done-with-concerns | blocked; confidence; spec_issues; deviations;
  files; verification; warnings) + marker-based parser over a child
  reply's trailing block + canonical renderer.
- `handoff(from, to, #{ report: true, ... })` in the flow API marks
  the resulting `DelegateTool` report-enabled; the delegation prompt
  appends the canonical block instructions so the child knows the
  contract.
- Post-hoc enforcement in `DelegateTool::run` for report-enabled
  handoffs: missing markers append `[enforce] missing status markers:
  …` lines to the tool result (parent model sees them) and the parsed
  outcome + enforcement result land in trace metadata
  (status/confidence only — never files or bodies). Fail-open: the
  child's text is never blocked or rewritten beyond appended warnings.

## Acceptance

Tests: a compliant reply parses into the typed struct; a
marker-missing reply gains `[enforce]` lines while its original text
survives verbatim; non-report handoffs are untouched; trace metadata
carries the parsed status. `cargo test -p codon-agent` green.
