---
id: TASK:phase-22/fish-context-injection
type: task
status: accepted
version: 0.1.0
summary: >
  Enrich the `agent.complete` server handler with the terminal's
  cwd, the pane-kind preamble, the project-kb directory summary
  (when available), and the last N matching command-history rows.
  Everything routed through the redaction pipeline at egress
  before the model sees it.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-context-injection
  - REQ:codon/fish-shell-integration#c-redact-at-egress
aspects: [context-assembly, egress-redaction]
blocked_by:
  - TASK:phase-22/fish-hash-at-trigger
  - TASK:phase-22/preamble-assembler
  - TASK:phase-22/project-kb-preamble
  - TASK:phase-22/redaction-pipeline-trait
---

# Context injection + egress redaction

## Plan

- The `agent.complete` server handler assembles the agent
  prompt in this order:
  1. `Preamble::build(workspace, cx)` — the standard preamble
     (`# codon-preamble v1` header, session, window, focused
     pane snippet). The pane is the terminal the fish RPC came
     from.
  2. **Project-kb directory summary** matching the terminal's
     cwd (via `codon_project_kb::for_path(cwd)`). When
     project-kb is disabled or no row matches, omit.
  3. **Recent command-history.** When command-history is
     enabled, fetch the last 20 entries whose `cwd` matches the
     terminal's cwd, ordered newest-first. Include their
     `summary_what` and exit_code. Skip rows with
     `llm_skipped = true`.
  4. **The user payload**: a structured message with
     `partial` and `description` as fields plus the
     `target_shell = "fish"` directive.
- **Egress redaction** is the last step before the harness
  call:
  - Assemble the prompt into a single `String`.
  - Run `codon_redact::default_pipeline().redact(prompt,
    RedactCtx { caller: "fish_complete", ... })`.
  - `Clean` or `Redacted` → wrap in `RedactedText`, pass to
    `Harness::run_turn`.
  - `Risky` → return an RPC error
    `{ code: "redaction_risky", message: "sensitive content
    detected" }`. The plugin prints the user-facing message
    and restores the buffer.
- The fish plugin DOES NOT send the user's `description` as a
  separate redaction call — the server-side assembly redacts
  the full prompt (including the user's text) as one unit so
  cross-cell patterns are caught.
- Trace integration: every `agent.complete` turn emits a
  trace entry with `flow: "fish_complete"`. The `RedactionEvent`
  records caller, spans, kinds, outcome (per the existing
  trace-redaction rules — no bodies).

## Acceptance

- A `#@` invocation from a terminal at `<workspace>/src/auth/`
  carries the directory summary for that cwd when project-kb
  has one.
- The recent-history block contains exactly the last 20
  matching-cwd entries (test with a seeded store).
- An `agent.complete` whose user description contains
  `AWS_SECRET_ACCESS_KEY=AKIA...` returns
  `redaction_risky`; the captured stub-model input is empty
  (no call was made).
- The trace shows `flow: "fish_complete"` + the redaction
  event for every invocation.
- `cargo test -p codon-fish` passes.
