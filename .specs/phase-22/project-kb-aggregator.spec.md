---
id: TASK:phase-22/project-kb-aggregator
type: task
status: accepted
version: 0.1.0
summary: >
  Build the directory + project rollup summary generator: collect
  the last N command-history rows + pinned memories within the
  time window, hash the inputs, call the summarizer LLM only when
  the hash differs, write the resulting paragraph to the
  `kb_summaries` table.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/project-knowledge-base#c-directory-summary
  - REQ:codon/project-knowledge-base#c-project-summary
  - REQ:codon/project-knowledge-base#c-aggregation-window
  - REQ:codon/project-knowledge-base#c-incremental-refresh
  - REQ:codon/project-knowledge-base#c-refresh-cadence
  - REQ:codon/project-knowledge-base#c-storage
  - REQ:codon/project-knowledge-base#c-redaction-prepass
  - REQ:codon/project-knowledge-base#c-cost-cap
  - REQ:codon/project-knowledge-base#c-trace
  - REQ:codon/project-knowledge-base#c-bounded-paragraph
aspects: [directory-aggregation, project-aggregation, window, incremental-hash, cadence-scheduler, storage-table, redaction-call, cost-share, trace-emit, paragraph-cap]
blocked_by:
  - TASK:phase-22/command-history-store
  - TASK:phase-22/harness-api
---

# Project KB aggregator (directory + project summaries)

## Plan

- New crate
  `crates/codon-project-kb/` with lib-root
  `src/codon_project_kb.rs`. Depends on `codon-command-history`
  (reads its store) and `codon-memory` (reads pinned memories).
- Storage: extend `codon-command-history`'s sqlite file with a
  new table:
  ```sql
  CREATE TABLE kb_summaries (
      scope TEXT NOT NULL,                -- 'directory' | 'project'
      path TEXT NOT NULL,                 -- absolute or workspace root
      body_markdown TEXT NOT NULL,
      inputs_hash TEXT NOT NULL,          -- sha256 hex
      generated_ts TEXT NOT NULL,
      model_used TEXT NOT NULL,
      tokens_used INTEGER NOT NULL,
      PRIMARY KEY (scope, path)
  );
  ```
- Aggregator API:
  ```rust
  pub struct Aggregator { /* refs to history + memory stores */ }
  impl Aggregator {
      pub fn refresh_directory(&self, path: &Path) -> Result<RefreshOutcome>;
      pub fn refresh_project(&self) -> Result<RefreshOutcome>;
      pub fn refresh_all(&self) -> Result<Vec<RefreshOutcome>>;
      pub fn get(&self, scope: Scope, path: &Path) -> Result<Option<KbSummary>>;
  }
  pub enum RefreshOutcome { Unchanged, Regenerated { tokens_used: u32 }, Skipped { reason: SkipReason } }
  ```
- Inputs hash: SHA-256 over a stable representation of the
  ordered list of input row IDs + their `ts_utc`. Same inputs →
  same hash → no LLM call.
- Refresh cadence:
  - Background task spawned on workspace open. First tick: on
    open if last refresh > 1 h. Subsequent: hourly.
  - The user-triggered `r` in the picker (sibling
    `project-kb-picker`) calls `refresh_directory` or
    `refresh_project` synchronously.
- Time window: `[project_kb] window_days = 14`. Aggregator
  ignores rows whose `ts_utc < now - window`.
- Redaction prepass: the inputs from command-history are
  already-redacted (stored that way). Memory bodies are
  also-already-validated. The aggregator MUST still call the
  pipeline once over the *concatenated* prompt — catches
  patterns that emerge only across cells. `Risky` outcome →
  `RefreshOutcome::Skipped { reason: RiskyAggregate }`.
- Summarizer call: same `Harness::run_turn` flow as
  command-history's summarizer but with a different prompt
  template at `crates/codon-project-kb/src/aggregator_prompt.md`.
  Returns a single markdown paragraph ≤ 2 KiB; truncated on
  overrun with a trace warning.
- Cost budget shared: the aggregator's tokens count against the
  `[command_history] daily_summarize_token_budget` (the user's
  config already has one knob; sharing keeps it simple). When
  exhausted, return `Skipped { reason: BudgetExhausted }`.

## Acceptance

- Refresh on a directory with N=20 history entries and 2 pinned
  memories produces a `kb_summaries` row with `scope='directory'`.
- Re-refresh with no input changes returns
  `RefreshOutcome::Unchanged`, no LLM call.
- Refresh on a directory with one more new entry returns
  `RefreshOutcome::Regenerated`.
- Risky aggregate input (test fixture concatenates two clean
  rows whose join matches a redactor pattern) → `Skipped {
  RiskyAggregate }`.
- Time-window test: rows older than `window_days` are excluded.
- `cargo test -p codon-project-kb` passes.
