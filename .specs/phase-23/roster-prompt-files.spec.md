---
id: TASK:phase-23/roster-prompt-files
type: task
status: accepted
version: 0.1.0
summary: >
  `prompt_file:` in flow agent declarations — resolved relative to the
  flow file, read at compile time, last-good on failure, exclusive
  with inline `prompt:`.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-roster#c-prompt-files
assignee:
eta:
blocked_by: []
---

# Roster prompt files

## Plan

- `parse_agent` in
  [routing.rs](spec:src:crates/codon-agent/src/runtime/routing.rs)
  accepts `prompt_file: "orchestrator.md"`; both `prompt:`/
  `system_prompt:` and `prompt_file:` present → compile error.
- Resolution: relative to the flow file's directory (the flow loader
  threads the resolved flow path into compilation); absolute and `~/`
  paths honored via the existing `expand_tilde`.
- The file is read at flow-compile time; a missing/unreadable file
  fails the load and the last-good registry stays live
  (REQ:codon/agent-routing-harness#c-last-good), with the error
  surfaced through the existing routing-error metadata.

## Acceptance

Tests: a flow whose agent uses `prompt_file` compiles with the file's
contents as system prompt; a missing file keeps the last-good flow and
sets a routing error; declaring both prompt forms fails compile with a
clear message. `cargo test -p codon-agent` green.
