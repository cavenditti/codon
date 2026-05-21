---
id: TASK:phase-22/command-history-retention-budget
type: task
status: accepted
version: 0.1.0
summary: >
  Implement the entry-count retention cap (`max_entries`,
  default 10k) with oldest-first GC, the daily-token-budget guard
  on summarizer LLM calls, and the harness-trace integration for
  every summarization turn.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-history#c-retention
  - REQ:codon/command-history#c-cost-cap
  - REQ:codon/command-history#c-workspace-scope
  - REQ:codon/command-history#c-trace
aspects: [entry-cap-gc, token-budget, workspace-scope, trace-events]
---

# Retention + token budget + trace

## Plan

- Retention:
  - `HistoryStore::gc(max_entries)` drops oldest rows above the
    cap. Pinned entries (added in sibling
    `command-history-search-and-picker`) are exempt.
  - Triggered on workspace open if last-GC > 24 h ago (timestamp
    persisted in a `meta` table). Also exposed as a manual
    `codon_command_history::GcNow` action for tests + power
    users.
  - Config: `[command_history] max_entries = 10000` default.
- Token budget:
  - The summarizer (sibling task `command-history-summarizer`)
    consults a `TokenBudget` shared with the harness's per-day
    counter (the same `total_tokens_out` accumulator from
    `harness-cost-bookkeeping`, scoped to the
    `command_history.summarize` flow tag).
  - Default `daily_summarize_token_budget = 50000`. Reset at UTC
    midnight (a tiny per-store table tracks the last-reset
    timestamp + accumulated spend).
  - When the budget is exhausted the summarizer marks pending
    entries `llm_skipped = true` and does not enqueue further
    LLM calls until the next reset.
- Workspace scope: every API call on `HistoryStore` carries the
  fingerprint; an attempt to construct a `HistoryStore` for a
  non-current workspace fingerprint from inside an agent tool
  returns `cross_workspace_denied`. Enforced at the
  `HistoryStore::open` callsite.
- Trace integration:
  - Each summarization turn emits a `PhaseEvent::ModelCallFinished
    { tokens_in, tokens_out, flow: "command_history.summarize" }`.
  - The redaction-prepass result (from
    `command-history-osc133-consumer`) emits a
    `RedactionEvent { caller: "command_history", outcome }`. Both
    visible in the TraceViewer picker from
    `harness-cancellation-and-trace`.

## Acceptance

- Inserting 10k+1 entries triggers GC; oldest-but-not-pinned
  drops; row count returns to ≤ 10k.
- A test that pre-fills the daily budget to (limit - 100 tokens)
  and inserts an entry whose estimated cost is 200 tokens marks
  the entry `llm_skipped` and emits no LLM call.
- Trace contains `command_history.summarize` ModelCallFinished
  entries after a successful summary.
- Reset-at-UTC-midnight test: advance the clock 25 h, insert a
  new entry, summary succeeds.
- `cargo test -p codon-command-history` passes.
