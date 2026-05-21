---
id: TASK:phase-22/pane-tools-command-history-search
type: task
status: accepted
version: 0.1.0
summary: >
  Implement the `search_command_history` agent tool: a thin
  adapter over `HistoryStore::search`, returning redacted entries
  with summaries. Off-feature behaviour returns
  `tool_disabled { feature: "command_history" }`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-pane-tools#c-tool-search-command-history
  - REQ:codon/command-history#c-search-tool
aspects: [tool-adapter, history-search-fts]
blocked_by:
  - TASK:phase-22/command-history-store
  - TASK:phase-22/harness-api
---

# Agent tool: search_command_history

## Plan

- New module
  `crates/codon-agent/src/tools/command_history.rs`.
- Tool shape:
  ```rust
  pub struct SearchCommandHistoryArgs {
      pub query: String,
      pub cwd: Option<PathBuf>,
      pub since: Option<DateTime<Utc>>,
      pub until: Option<DateTime<Utc>>,
      pub limit: Option<u32>,
  }
  pub struct SearchCommandHistoryResult {
      pub entries: Vec<CommandHistoryHit>,
      pub truncated: bool,
  }
  pub struct CommandHistoryHit {
      pub ts: DateTime<Utc>,
      pub cwd: String,
      pub command_text: Option<String>,   // redacted-for-egress; None if llm_skipped
      pub summary_what: Option<String>,
      pub summary_did: Option<String>,
      pub exit_code: Option<i32>,
      pub llm_skipped: bool,
      pub skip_reason: Option<String>,
  }
  ```
- Implementation:
  - Check `[command_history] enabled`. When false:
    `tool_disabled { feature: "command_history" }`. Returned as
    a structured tool error the harness surfaces back to the
    model.
  - Open the workspace's `HistoryStore` (singleton per
    workspace, kept on the workspace state).
  - Call `HistoryStore::search` with the args. Default `limit
    = 20`; max enforced 100 to prevent budget exhaustion.
  - **This tool is an LLM egress point.** Each row's
    `command_text` is run through
    `codon_redact::default_pipeline().redact(...)` before being
    placed in the result. The redacted string is what reaches
    the model.
    - `Clean` → use the original (no secrets found).
    - `Redacted` → use the redacted text with placeholders.
    - `Risky` → set `command_text: None`, populate
      `llm_skipped: true`, `skip_reason: "risky_redaction"`.
      The model sees only the metadata; the raw row remains in
      the store and visible to the user via the picker.
  - Rows that already carry `llm_skipped = true` in the store
    (because the summarizer already saw them as Risky) are
    surfaced with `command_text: None` directly — no second
    redactor pass needed.
- Workspace scope: the harness only ever calls the tool against
  the active workspace's store. The pane-tools workspace-scope
  guard already covers this since the tool is registered
  per-workspace.

## Acceptance

- A turn whose model calls `search_command_history { query:
  "deploy" }` returns matching entries when the feature is
  enabled and the store has matching rows.
- With `[command_history] enabled = false` the tool returns
  `tool_disabled`.
- Limit > 100 is silently clamped to 100; a trace warning is
  emitted.
- **Egress test**: a row whose raw `command_text` is
  `AWS_SECRET_ACCESS_KEY=AKIA...` is matched by a search; the
  returned `command_text` contains `<REDACTED:aws_key>`, never
  `AKIA`. The row in the store still has the raw bytes (verified
  by a parallel direct read).
- `llm_skipped` rows in the store come through with
  `command_text: None` and `llm_skipped: true`, so the model
  knows not to reference content.
- `cargo test -p codon-agent` passes.
