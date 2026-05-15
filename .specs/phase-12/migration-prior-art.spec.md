---
id: TASK:phase-12/migration-prior-art
type: task
status: accepted
version: 0.0.1
summary: >
  Resolve the two prior per-panel conversion attempts (agent pane,
  git status pane) against the new adapter-driven model — without
  losing the valid pieces of each.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-migration-prior-art
---

# Reconcile with prior per-panel attempts

## What changes

Three earlier spec entries get cross-referenced and partially
resolved by Phase 12:

### [TASK:phase-3/agent-pane-conversion](spec:TASK:phase-3/agent-pane-conversion)
**Was:** `deferred` — file too large, conversion too invasive.
**Now:** the adapter eliminates the per-panel rewrite. Once
[TASK:phase-12/agent-panel-migration](spec:TASK:phase-12/agent-panel-migration)
lands and wires `AgentPanel` through `PanelItemAdapter`, this task
is marked `done` (the *outcome* it described — agent reachable as a
pane — is achieved). The spec body of phase-3 stays as a historical
note; the `done` transition is the resolution.

### [TASK:phase-4/git-panel-modal-integration](spec:TASK:phase-4/git-panel-modal-integration)
**Stays as-is.** The dispatch-context patches, `CodonModeTracker`
wiring, and `[bindings.git_panel.*]` keymap blocks remain valid for
*both* placements: the panel still publishes `pane_mode = "normal"`
/ `"insert"` whether hosted by the adapter (as a pane) or by the
peek surface (as a transient dock). No spec-state change needed.

### [TASK:phase-4/git-status-pane](spec:TASK:phase-4/git-status-pane)
**Stays `wontdo`.** The original approach (re-implement the status
view from scratch as `crates/codon-git`) is still the wrong answer
— the lesson learned ("don't duplicate ~6000 lines of working git
panel code") is exactly what `PanelItemAdapter` operationalizes.
The clause it was meant to satisfy
(`REQ:codon/git-pane#c-status`) is re-satisfied via the adapter,
recorded in [TASK:phase-12/git-panel-migration](spec:TASK:phase-12/git-panel-migration).

## Approach

This task is editorial: when the four migration tasks under
phase-12 are complete, run `spec done` on
`phase-3/agent-pane-conversion` and update the body with a
back-pointer to phase-12. No spec-state change for the two phase-4
tasks (modal-integration stays `done`; status-pane stays `wontdo`).

## Non-goals

- No code changes. This task is purely about keeping the spec graph
  honest as Phase 12 lands.
