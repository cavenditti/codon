---
id: REQ:codon/command-history
type: requirement
status: draft
version: 0.1.0
level: SHOULD
summary: >
  An indexed, AI-summarized history of every command that executes
  in codon's terminals. Built on the OSC 133 boundary events from
  REQ:codon/terminal-blocks; each completed command becomes an
  entry with redacted body, what-it-does + what-it-did summaries,
  exit code, cwd, and tags. Searchable as an agent tool and via a
  picker; surfaces into the preamble through
  REQ:codon/project-knowledge-base. Opt-in per workspace because
  summarization fires LLM calls.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Command history

## Context

A terminal-first editor accumulates an enormous, structured signal:
every prompt, every command, every exit code, every output region.
Phase 19's OSC 133 work (REQ:codon/terminal-blocks) already
identifies those boundaries deterministically. What's missing is
the consumer: a store that ingests those events, an async
summarization step that turns each command into searchable
natural language, and the surfaces the user (and the agent)
interact with.

Two motivating workflows:

1. "What was that command I ran last week to dump the staging DB?"
   — currently solved with `Ctrl-R` over shell history, which
   finds the command if you remember a substring of it but misses
   when you only remember the *intent* ("the staging dump").
   A natural-language summary indexed alongside the command makes
   the search a one-shot.
2. The agent asks "what's been happening in this directory?" — at
   the moment the answer is nothing. With a per-directory rollup
   (delivered by REQ:codon/project-knowledge-base), the agent has
   actual context for "this is the worktree where you're
   refactoring the auth middleware; you last ran the test suite
   2 hours ago; it failed on a flaky DB fixture."

Trust boundary and design constraints:

- **The store holds raw command bytes and raw output excerpts.**
  Same trust model as the user's shell history file
  (`~/.zsh_history`, `~/.bash_history`,
  `~/.local/state/fish/fish_history`) — the disk is the user's
  trust boundary. Redaction does NOT happen on ingress; the
  picker, the paste-to-PTY path, and the local user reading the
  sqlite file all see the bytes as typed.
- **The threat we protect against is egress to LLM providers.**
  Every byte that crosses into a model call — the summarizer's
  prompt, an agent-tool's return value to the model, anything
  that ends up in the agent's context — MUST go through the
  redaction pipeline from
  [REQ:codon/secret-redaction-pipeline](spec:REQ:codon/secret-redaction-pipeline)
  first. That's the hard invariant. The redactor's fail-closed
  mode means a row whose contents the redactor can't confidently
  clean for egress is marked `llm_skipped = true`; the row
  remains in the store (still searchable, still pasteable) but
  the summarizer is never invoked on it.
- **The summarization feature is opt-in.** Codon's default is
  no LLM calls. The user enables
  `[command_history] enabled = true` per workspace (or globally).
  The first time the user enables it codon shows a one-pane
  onboarding explaining what gets sent to the LLM (the redacted
  prompt) and which redactor + model is in use (Presidio offline
  + the harness model by default). Note: the store itself can
  exist without summarization — a future "history index only,
  no LLM" mode is fine, deferred for now to keep the opt-in
  story simple.

The store is sqlite, not a directory of markdown — volume is
hundreds of entries per day, not handfuls of curated notes. It
sits alongside the memory store (REQ:codon/agent-shared-memory)
but has a different trust model: memories require an explicit
user-confirm to write; command-history accrues automatically
(asking on every command would be insufferable). The two should
not be conflated in one store.

:::{requirement id="command-history" level="SHOULD"}
The system SHOULD provide:

- {#c-source-osc133} command-history MUST consume boundary events
  from
  [REQ:codon/terminal-blocks](spec:REQ:codon/terminal-blocks)
  (`Block` start, command text, exit code, output region) — no
  re-parsing of PTY bytes. A terminal whose shell isn't emitting
  OSC 133 falls through to the heuristic detector (also from
  terminal-blocks) and produces lower-fidelity entries that omit
  exit code
- {#c-store-sqlite} a workspace-scoped sqlite store at
  `~/.config/codon/command_history/<fingerprint>.sqlite` (same
  workspace fingerprint as REQ:codon/agent-shared-memory#c-fingerprint).
  Schema: `entries(id, ts_utc, cwd, shell, command_text,
  output_excerpt, exit_code, duration_ms, summary_what,
  summary_did, tags_json, llm_skipped, skip_reason)` + FTS5
  virtual table over `command_text + summary_what + summary_did
  + tags`. `command_text` and `output_excerpt` are raw bytes
  (same trust model as shell history)
- {#c-redact-on-llm-egress} every byte that crosses into an LLM
  call MUST first pass through the pipeline from
  REQ:codon/secret-redaction-pipeline. Egress points: (a) the
  summarizer prompt assembly, (b) the `search_command_history`
  agent tool's return value, (c) any future flow that includes
  command-history rows in a model context. The redactor's
  output — not the stored raw bytes — is what reaches the model.
  The redacted forms are computed at egress time and NOT
  persisted; the store itself stays raw
- {#c-llm-skip-on-risky} if the redactor returns
  `RedactionOutcome::Risky` for a row's contents at summarization
  time, the summarizer is NOT invoked: the row is updated with
  `llm_skipped = true` and `skip_reason = "risky_redaction"`.
  Future summarization attempts on the same row also skip. The
  row remains in the store with its raw bytes intact — the
  picker shows it normally, the user can still paste / yank
  it; only the LLM-bound surfaces (`search_command_history` for
  the agent, the project-kb aggregator) honour the skip and
  exclude the row from results destined for the model
- {#c-summarize-async} summarization is async + queued: a
  completed command enqueues a job; the worker calls the
  summarizer LLM out-of-band; the result is written back to the
  same row. The user's terminal never blocks on summarization.
  The queue is debounced — bursts (e.g. a long pipeline of
  `cd && ls && grep`) coalesce into one combined entry when the
  commands run within a 500 ms window of each other and share a
  cwd
- {#c-summary-shape} `summary_what` is "what this command does"
  (one sentence, present tense), `summary_did` is "what it did
  this time" (one sentence, past tense, mentions exit code +
  any salient stderr/stdout pattern the redactor allowed
  through). Both are ≤ 280 chars. Tags are 0-5 short keywords
  extracted by the same call
- {#c-search-tool} a `search_command_history { query, cwd?,
  since?, until?, limit? }` agent tool exposed via the harness
  (extends REQ:codon/agent-pane-tools). Backed by sqlite FTS5
  over (command_text + summary_what + summary_did + tags). Returns
  entries ordered by relevance then recency
- {#c-picker} a `codon_command_history::HistoryPicker` modal —
  proposed bind `prefix h` — over the workspace's entries.
  Columns: time, cwd, summary (falls back to the raw command
  text when `summary_what` is NULL). Enter pastes the raw
  command at the focused terminal's PTY cursor (no execute);
  `y` yanks the raw command; `e` opens a detail view with both
  summaries and the raw output excerpt. The picker is a
  local-user surface — no redaction on this path
- {#c-retention} retention policy: keep at most
  `[command_history] max_entries = 10000` entries per workspace.
  Oldest entries drop first when the cap is hit. A nightly GC
  job (run on workspace open if last-run > 24 h ago) compacts
  the sqlite file. The cap is configurable
- {#c-cost-cap} an LLM-call budget: `[command_history]
  daily_summarize_token_budget = 50000` (default; configurable).
  When the budget is exhausted, new entries are stored without
  summaries (`llm_skipped = true`) until the next UTC day.
  The status bar item from
  REQ:codon/agent-harness#c-cost-bookkeeping surfaces the burn
  rate when the token counter is enabled
- {#c-opt-in} the feature is OFF by default. Enabling requires
  `[command_history] enabled = true` in `codon.toml`. The first
  time the user flips it on, codon opens a one-pane onboarding
  describing what gets stored, where, and which redactor + model
  is being used. A second-time-on is silent
- {#c-workspace-scope} the store is per-workspace; the picker
  and the search tool never expose entries from other
  workspaces. Cross-workspace export/import is deferred (the
  shared-memory export/import pattern is the template if it ever
  ships)
- {#c-trace} every summarization turn appears in the harness
  trace (REQ:codon/agent-harness#c-trace) with the redaction
  outcome (`clean`, `redacted_count`, `risky_skipped`) but
  without bodies — same redaction rule as the rest of the trace
- {#c-no-llm-leak} no agent tool, summarizer prompt, or any
  other LLM-bound surface that includes command-history content
  may pass raw bytes to the model. Tests verify this by injecting
  a known secret pattern into a synthetic terminal, running the
  summarization + agent-tool path, and asserting the captured
  model-client input contains only the redacted placeholder.
  The sqlite store IS allowed to contain the raw bytes — this
  test is about egress, not persistence
:::

## Out of scope

- Shell-history shadow file. Codon does not replace the user's
  `~/.bash_history` / `~/.zsh_history` / `~/.local/state/fish/
  fish_history`; those continue to work. Codon's store is a
  parallel index for *search* and *summarization*.
- Live "as-you-type" suggestions. The agent's path for that is
  REQ:codon/agent-contextual-suggest's terminal shape — the
  history index *informs* those suggestions through the agent
  tool, but the suggestion itself flows through the existing
  confirm-overlay (no auto-prefill from history alone).
- Cross-machine sync. A user with codon on a desktop and a
  laptop has two stores; export/import is deferred.
- A "favourite commands" list. The pinned mechanic in
  REQ:codon/agent-shared-memory already covers that use case
  (memory body = the command itself; pinned = surfaces in the
  preamble).
