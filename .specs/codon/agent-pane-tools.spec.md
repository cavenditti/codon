---
id: REQ:codon/agent-pane-tools
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  A bounded set of tools the agent can call during a turn to inspect
  the workspace and shape its reply. Read-only pane inspection
  (current + named other panes), a list-panes enumerator, and three
  reply-shaping tools (`suggest_command`, `suggest_action`,
  `suggest_response`). All tools are scoped to the active workspace,
  all read tools are byte-budgeted, and the reply-shaping tools are
  gated by the pane router from agent-contextual-suggest.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Agent pane tools

## Context

Phase 22's contextual-suggest verb gives the agent one user
question plus the standard preamble. For anything beyond a one-shot
answer, the model needs to look around. Today there is no surface
for "look at the rest of the terminal scrollback" or "what's the
file path of the other split". Adding it ad-hoc per feature would
guarantee divergence.

The pane-tools layer is the single ground truth for what the agent
can inspect and how it can shape its reply. Tools are registered
once against the harness from
[REQ:codon/agent-harness](spec:REQ:codon/agent-harness) and reused
across contextual-suggest, the existing cross-pane verbs, and any
future agent flow. Two design principles drive the shape:

1. **Each pane kind implements one trait.** `PaneInspect` exposes
   `read_visible`, `read_scrollback`, `search`, `summary`, and
   `kind_label`. Every tool routes through that trait, so adding a
   pane kind (today or later) doesn't grow the tool surface.
2. **Inspection is read-only and budgeted.** No tool the agent calls
   here mutates the workspace. Every read returns at most a
   configured byte budget (default 8 KB per call, configurable in
   `codon.toml`). Truncation is explicit in the returned payload
   (`truncated: true` + final byte offset) so the model can ask for
   the next range deterministically.

The pane router from agent-contextual-suggest gates the
reply-shaping tools by pane kind. `suggest_command` is only legal
when the active pane kind is `terminal`; calling it from any other
pane returns a tool error the harness surfaces to the model so it
can retry with a legal shape.

:::{requirement id="agent-pane-tools" level="MUST"}
The system MUST provide:

- {#c-pane-inspect-trait} a `PaneInspect` trait in
  `crates/codon-pane-bridge` with read-only methods
  `kind_label() -> &str`, `summary(cx) -> PaneSummary`,
  `read_visible(cx, byte_budget) -> PaneSlice`,
  `read_scrollback(cx, byte_budget, offset) -> PaneSlice`, and
  `search(cx, pattern, byte_budget) -> Vec<SearchHit>`. Implemented
  by terminal, editor, file_manager, agent, outline, git, debug,
  peek (no-op default for kinds without content)
- {#c-tool-grep-current} a `grep_current_pane(pattern, max_hits?)`
  tool — resolves the focused pane, runs `PaneInspect::search`
  against it, returns hits with surrounding context lines. Errors
  clearly when the pane kind returns the no-op default
- {#c-tool-read-current} a `read_current_pane(scrollback?, offset?)`
  tool — returns the active pane's visible region by default; with
  `scrollback=true` returns scrollback from `offset`. Honours the
  byte budget; sets `truncated` + `next_offset` when more remains
- {#c-tool-list-panes} a `list_panes()` tool — returns a flat list
  of every pane in the active window with kind, slot/tab label, and
  a short summary. Lets the agent decide which other pane to query
- {#c-tool-grep-other-pane} a `grep_pane(pane_id, pattern,
  max_hits?)` tool — same shape as grep_current_pane but takes a
  `pane_id` from `list_panes`. Rejects pane IDs not in the current
  window
- {#c-tool-read-other-pane} a `read_pane(pane_id, scrollback?,
  offset?)` tool — same shape as read_current_pane against a named
  pane. Same byte budget + truncation rules
- {#c-tool-suggest-command} a `suggest_command(command, why)` tool
  — legal only when the active pane is a terminal. Payload is the
  literal shell command (single-line preferred; multi-line allowed
  for here-doc / pipe chains) plus a one-sentence rationale. The
  host renders the confirm-overlay from
  REQ:codon/agent-contextual-suggest#c-terminal-command
- {#c-tool-suggest-action} a `suggest_action(action_name,
  payload_json?, why)` tool — legal in editor and file_manager
  panes. `action_name` MUST resolve through Zed's global action
  registry; an unresolved name is a tool error. The host previews
  the action in the overlay; the user confirms to dispatch
- {#c-tool-suggest-response} a `suggest_response(text)` tool —
  legal in every pane. Renders read-only text in the overlay
- {#c-router-enforcement} the pane router (from
  REQ:codon/agent-contextual-suggest#c-pane-router) is the single
  authority on which reply-shaping tool is legal. The harness asks
  the router on every tool call and returns a structured tool
  error on mismatch — the agent can retry with a legal shape
- {#c-byte-budget} every read/grep tool honours a per-call byte
  budget (default 8 KB, configurable as
  `[agent_tools] read_byte_budget = ...` in `codon.toml`).
  Truncation is signalled, never silent. The total budget across
  one agent turn is also bounded (default 64 KB; configurable as
  `[agent_tools] turn_byte_budget`) so a runaway tool loop can't
  exhaust the context
- {#c-workspace-scope} no tool returns data from outside the active
  workspace root. Pane IDs from other windows / other workspaces
  are rejected. The shared memory tool from
  REQ:codon/agent-shared-memory follows the same workspace-scoping
  rule
- {#c-read-only} every inspection tool is read-only. Mutating
  effects (sending text to a PTY, dispatching an action) happen
  only through the reply-shaping tools, and only after the user
  confirms in the overlay
- {#c-cancellation-aware} tools respect the harness cancellation
  signal: a long search returns early when the user cancels the
  turn (see REQ:codon/agent-harness#c-cancellation)
- {#c-tool-search-command-history} a
  `search_command_history { query, cwd?, since?, until?, limit? }`
  tool exposed via the harness — backed by the FTS5 index from
  [REQ:codon/command-history](spec:REQ:codon/command-history)#c-search-tool.
  Returns redacted command entries with their summaries (never
  raw bodies). Available only when command-history is enabled
  in `codon.toml`; otherwise returns
  `tool_disabled { feature: "command_history" }` so the model
  can fall back to other context paths
:::

## Out of scope

- File-system tools (`read_file`, `write_file`, `run_shell`). The
  agent already has these via the upstream Zed agent_ui surface for
  the dedicated agent pane. Contextual-suggest is intentionally
  narrower — pane inspection only — to keep the suggestion turn
  fast and obviously read-only.
- Editing tools (insert/replace/diff). The reply-shaping tools
  return *suggestions*; the host applies them only after user
  confirmation. There is no agent-driven edit tool in phase 22.
- Network tools. The agent already has whatever HTTP surface the
  upstream agent crate provides; phase 22 does not add network
  inspection.
