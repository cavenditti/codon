---
id: TASK:phase-23/shell-gate-escalation
type: task
status: accepted
version: 0.1.0
summary: >
  Two-model deny escalation: a classifier deny is re-examined by a
  configured second-opinion agent; sensitive categories and high risk
  block the override; double-deny resolves to `ask`.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-shell-safety#c-deny-escalation
assignee:
eta:
blocked_by: []
---

# Shell gate escalation

## Plan

- Extend the flow API: `safety_for("shell", primary)` keeps its shape;
  a three-arg overload `safety_for("shell", primary, escalation)`
  registers a second-opinion agent (both validated against declared
  agents at compile).
- Port `applyEscalationPolicy` from the reference plugin to
  `runtime/safety.rs`: second opinion allows AND no sensitive category
  (`destructive|irreversible|secret|credential|exfiltrat|privilege`)
  from either pass AND second-opinion risk < 50 → `allow` with
  `escalation-override` category; otherwise `ask` with
  `escalation-degraded` / `double-deny-escalation` category. Reasons
  from both passes are merged into the final verdict reason.
- The second-opinion prompt reuses the contract prompt with a
  second-opinion framing line (re-examine independently, hard-deny
  categories still binding).
- No escalation agent configured → classifier deny resolves to `ask`.

## Acceptance

Unit tests cover the full matrix: deny→allow override, deny→allow with
scary category → ask, deny→allow risk ≥ 50 → ask, deny→deny → ask,
escalation agent unavailable → ask, no escalation configured → ask.
`cargo test -p codon-agent` green.
