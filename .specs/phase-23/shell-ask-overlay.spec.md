---
id: TASK:phase-23/shell-ask-overlay
type: task
status: accepted
version: 0.1.0
summary: >
  Keyboard-first one-shot approval overlay for `ask` safety verdicts —
  command, risk, categories, reason shown; Enter approves once, Esc
  denies; no persistent allowlist.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shell-safety#c-ask-decision
assignee:
eta:
blocked_by: []
---

# Shell ask overlay

## Plan

- New approval modal on `codon-pickers::ModalScaffold` showing the
  pending command verbatim, the verdict's risk / categories / reason,
  and the requesting agent. Enter approves this invocation only; Esc
  (or dismiss) denies. One-shot per REQ — approving never writes a
  persistent rule.
- Route `ask` verdicts from `ShellCommandTool` through a
  foreground-dispatched request that opens the overlay on the active
  workspace and resolves the awaiting tool future; turn cancellation
  dismisses the overlay and resolves to deny.
- Overlay visibility integrates with the mode tracker as a global
  transient (extend PaneMode / `*_active` override on
  `CodonModeTracker` per the established pattern — no parallel
  indicator).
- Remove the interim ask→refusal fallback from the verdict path once
  the overlay resolves asks for real.

## Acceptance

Opening/approving/denying is fully keyboard-driven; an approved
command executes exactly once; Esc and turn-cancel both deny; a GPUI
test drives the overlay end-to-end with a stub verdict.
`cargo test -p codon-agent` green.
