---
id: REQ:codon/project-knowledge-base
type: requirement
status: draft
version: 0.1.0
level: SHOULD
summary: >
  Per-directory and per-project rollup summaries built from
  command-history entries and curated memories. A directory summary
  answers "what's been happening in this cwd"; a project summary
  answers "what's been happening in this workspace". Surfaces into
  the agent preamble when cwd matches; viewable through a picker.
  Same opt-in gate as command-history because new summaries fire
  LLM calls.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Project knowledge base

## Context

A command-history index makes individual past commands findable.
The next step up is *aggregation*: a paragraph that says "in this
directory, over the last week, you've been hardening the auth
middleware — twelve test runs (last one failed on a fixture), a
migration draft, and a Slack-share of the diff with @alice." That's
the kind of context that turns the agent from "what flag does
`pg_dump` take" into "you've been writing this migration in three
sessions across two days; here's where you left off." The
contextual-suggest verb gets dramatically more useful when the
preamble carries that paragraph.

Aggregation is generated artifact, not a freshly-curated thing per
event. It refreshes on a cadence (configurable; default: hourly +
on workspace open if last-refresh > 1 h ago) and only when the
inputs have changed. The build looks at three streams:

1. The last N relevant command-history entries (filtered by cwd
   for directory summaries, unscoped for project summaries) from
   REQ:codon/command-history.
2. The pinned memories from REQ:codon/agent-shared-memory.
3. (Optionally, when present) the codon-session window + tab labels
   for the workspace — a cheap signal for "what was the user
   actually thinking about".

The summarizer LLM is the same harness path as command-history
summarization; the redaction pipeline is the same. The output is a
≤ 2 KiB markdown paragraph stored alongside its inputs' hash so the
refresh is incremental — if nothing relevant changed, we don't
re-summarize.

Two surfaces:

- **Preamble.** When the focused pane's cwd matches a directory
  with a recent summary, the directory summary surfaces into the
  preamble's memory section (replaces or augments the
  keyword-matched memories from
  REQ:codon/agent-context-preamble#c-memories-budgeted depending on
  budget).
- **Picker.** `codon_project_kb::Picker` (proposed bind
  `prefix shift-h`) opens a fuzzy picker over every summary —
  directory summaries appear once per directory; the project
  summary appears once at the top. Enter opens the summary in a
  read-only buffer; `r` triggers an immediate refresh; `d` deletes
  (next refresh will rebuild).

:::{requirement id="project-knowledge-base" level="SHOULD"}
The system SHOULD provide:

- {#c-directory-summary} a per-directory rollup summary: a single
  paragraph (≤ 2 KiB markdown) describing the recent activity in
  a given absolute path. Built from the last N
  command-history entries whose `cwd` matches the directory
  (default N = 50, configurable), plus tagged memories
  pinned under that directory
- {#c-project-summary} a per-workspace rollup summary: same shape
  as the directory summary but unscoped on cwd. Built from the
  workspace's command-history (across all cwds) plus the
  workspace-pinned memories
- {#c-aggregation-window} both summaries respect a time window
  `[project_kb] window_days = 14` (default; configurable). Older
  command-history entries are excluded from the inputs even if
  they're still in the retention cap
- {#c-incremental-refresh} the summary's stored alongside the
  hash of its inputs (`SHA-256` over the input row IDs +
  timestamps). On refresh, if the hash matches the current
  inputs' hash, no LLM call is made — the existing summary is
  reused. A guaranteed-cheap default
- {#c-refresh-cadence} refresh runs (a) on workspace open if
  last-refresh > 1 h ago, (b) hourly while the workspace is open
  if inputs have changed, and (c) on explicit user trigger via
  `r` in the picker. Never on every command — debounced
- {#c-redaction-prepass} the summarizer call MUST route through
  the redaction pipeline from
  REQ:codon/secret-redaction-pipeline before any input is sent.
  Risky inputs are dropped (consistent with command-history's
  fail-closed rule)
- {#c-storage} summaries persist in the same sqlite store as
  command-history under a separate table:
  `kb_summaries(scope, path, body_markdown, inputs_hash,
  generated_ts, model_used, tokens_used)`. Keyed on
  `(scope, path)` where `scope` is `"directory"` or `"project"`
  and `path` is the absolute path (or the workspace root for
  project scope)
- {#c-preamble-surface} when a directory summary exists for the
  focused pane's cwd (or a parent up to the workspace root, with
  the most-specific match winning), it surfaces into the
  preamble's memory section per
  REQ:codon/agent-context-preamble#c-memories-budgeted. The
  project summary surfaces when no directory summary applies
- {#c-picker} a `codon_project_kb::OpenPicker` action — proposed
  bind `prefix shift-h` — opens a fuzzy picker listing every
  summary. Project summary pinned at the top; directory
  summaries below sorted by path. Enter opens the summary in a
  read-only buffer; `r` refreshes; `d` deletes
- {#c-opt-in} this feature shares the
  `[command_history] enabled` opt-in: enabling history enables
  the rollup pipeline. A user who wants history but not rollups
  can additionally set `[project_kb] enabled = false` to
  disable summary generation while keeping the per-command
  summaries
- {#c-cost-cap} rollup LLM calls draw from the same daily token
  budget as command-history. The budget is shared — running out
  affects both surfaces equally
- {#c-trace} every rollup turn appears in the harness trace with
  scope + path + tokens-used + redaction outcome
- {#c-bounded-paragraph} the summary's body is hard-capped at
  2 KiB. A summarizer that returns longer text gets the response
  truncated server-side and a trace warning emitted
:::

## Out of scope

- Cross-project insights ("you keep doing X across all your
  workspaces"). Per-workspace only for phase 22.
- Time-series charts / graphs of activity. The summary is text.
- Replaying past command sessions. The agent can search the
  command history; it cannot re-execute.
- A "summary diff" view (this week vs. last week). Useful but
  deferred.
