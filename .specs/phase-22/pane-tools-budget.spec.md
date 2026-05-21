---
id: TASK:phase-22/pane-tools-budget
type: task
status: accepted
version: 0.1.0
summary: >
  Enforce per-call and per-turn byte budgets on every pane-read /
  pane-grep tool. Make tools cancellation-aware so a long search
  returns early when the user cancels the turn.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-pane-tools#c-byte-budget
  - REQ:codon/agent-pane-tools#c-cancellation-aware
aspects: [byte-budget, cancellation-polling]
---

# Pane tools byte budget + cancellation

## Plan

- Add `[agent_tools]` table to `codon-config`:
  - `read_byte_budget = 8192` (per-call cap).
  - `turn_byte_budget = 65536` (per-turn cap across all tool calls).
- The tool dispatcher (lives in the harness, supplied by sibling
  task `harness-api`) holds a `TurnBudget { remaining: usize }`
  threaded through every tool call. Each tool consumes from
  `min(per_call_cap, remaining)`. When `remaining == 0` further
  tool calls return `turn_budget_exhausted` immediately.
- `PaneInspect::read_scrollback` and `PaneInspect::search` take a
  `byte_budget: usize` and stop reading once the budget is hit. The
  returned `PaneSlice` reports `truncated: true` + `next_offset` so
  the model can continue if there's per-turn budget left.
- Cancellation: every tool call receives the same `CancelToken` the
  harness holds. Long loops poll it (every N lines for search,
  every chunk for read). On cancel, the tool returns
  `tool_cancelled` and the harness propagates the turn cancellation.

## Acceptance

- A search over a synthetic 1 MB scrollback with
  `read_byte_budget = 8192` returns a `truncated` result whose
  `bytes.len() ≤ 8192`.
- Three back-to-back reads on the same turn exhaust the per-turn
  budget; the fourth returns `turn_budget_exhausted`.
- Cancelling mid-search returns `tool_cancelled` within 50 ms.
- `cargo test -p codon-agent` passes.
