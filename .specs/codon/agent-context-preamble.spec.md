---
id: REQ:codon/agent-context-preamble
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  A deterministic, byte-budgeted preamble injected ahead of every
  agent turn (contextual-suggest, the existing cross-pane verbs, and
  any future flow). Encodes the small set of facts the agent should
  always know about the session — workspace root, focused pane kind,
  pane mode, session/window labels, the pane's own minimal snapshot,
  and a one-line selection summary if present. Deterministic-by-state
  so prompt caching is effective; extensible via a trait so new pane
  kinds opt in without growing the preamble assembler.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Agent context preamble

## Context

The three existing cross-pane verbs each build their own prompt
prefix. Reviewing them side by side surfaces both duplication and
omission: the explain verb mentions the file path, the refactor
verb mentions the language, neither mentions the session, none
mention the pane mode. A user asking "what does this do" in a
terminal vs. in a buffer vs. in the FM is in three different
contexts and the agent currently has to infer that from the pasted
selection alone.

The preamble REQ is the single ground truth for "what does the
agent always know". One assembler — `codon_agent::Preamble` — runs
ahead of every agent turn. Each pane kind contributes a
`PaneSnapshot` via a trait. The assembler concatenates a fixed
ordering (session > window > pane > selection > memories), enforces
the byte budget, and emits a deterministic UTF-8 string.

Determinism is load-bearing. Two consecutive turns with identical
pane state must produce identical preamble bytes, so the
Anthropic / OpenAI cache prefixes hit. That excludes timestamps,
random IDs, and hashmap iteration order from the output. State that
changes turn-to-turn (e.g. selection bounds in a moving editor) is
included only when it actually changed.

Memories from
[REQ:codon/agent-shared-memory](spec:REQ:codon/agent-shared-memory)
are optionally surfaced into the preamble when small and pinned
(`pinned: true`) or matched by a cheap keyword filter against the
user's question. The budget enforcement clause caps how much memory
content can land in the preamble, so a large pinned set never
crowds out the pane snapshot.

:::{requirement id="agent-context-preamble" level="MUST"}
The preamble assembler MUST:

- {#c-assembler-api} expose a single entry point
  `codon_agent::Preamble::build(workspace, cx) -> String`. Every
  agent flow (contextual-suggest, AgentExplain/Summarize/Refactor,
  and any future flow) MUST call this — no flow assembles its own
  prefix
- {#c-byte-budget} the final string MUST be ≤ 2 KiB by default
  (configurable as `[agent_preamble] byte_budget = ...` in
  `codon.toml`). When sections push past the budget, lower-priority
  sections are dropped in order: memories first, then
  selection-detail, then pane scrollback hints, never the
  identifying header
- {#c-fixed-ordering} sections appear in a fixed order: (1)
  identifying header (workspace root, codon version), (2) session
  + window label, (3) focused pane kind + mode + slot, (4)
  pane-kind-specific snapshot, (5) selection summary if present,
  (6) surfaced memories if any. Sections never reorder
- {#c-deterministic} given identical workspace state, two
  consecutive calls MUST produce byte-identical output. No
  timestamps, no random IDs, no nondeterministic iteration. A
  property test asserts this
- {#c-pane-snapshot-trait} a `PaneSnapshot` trait on every pane
  kind returns a small (≤ 256 byte target) kind-specific summary:
  terminal → cwd + shell + last prompt-exit code from
  codon-terminal-blocks; editor → file path + language + cursor
  line; file_manager → cwd + marked count + selection range;
  agent → conversation turn count; outline → focused symbol; git
  → branch + dirty count; debug → stopped frame; peek → kind of
  the peeked panel
- {#c-selection-summary} when a selection is present, the
  selection-summary section is one line: `selection: <kind>
  (<n> bytes, <m> lines)` with `<kind>` from `SelectionSource`.
  The full selection text is *not* in the preamble — it goes into
  the user-message body if the flow needs it
- {#c-memories-budgeted} surfaced memories occupy at most 25% of
  the byte budget (rounded down). Pinned memories take priority,
  then keyword-filtered matches against the user's question. The
  memory section MAY be empty
- {#c-extensible} adding a new pane kind is one trait impl + one
  registry entry. The assembler MUST NOT contain a per-pane-kind
  match — pane snapshots come through the trait
- {#c-version-marker} the first line of the header is
  `# codon-preamble v1`. A bump (`v2`) accompanies any breaking
  change to the section ordering or section bodies — so an agent
  reading the preamble can detect and adapt
- {#c-no-secrets} the preamble MUST NOT include environment
  variables that look like secrets (`*_TOKEN`, `*_KEY`, `*_SECRET`,
  `*_PASSWORD`, plus the configurable
  `[agent_preamble] redact_env_patterns` list). The terminal pane
  snapshot's cwd is allowed; its env dump is not (and is not part
  of the snapshot to begin with)
- {#c-cheap-to-build} the assembler MUST complete in < 5 ms on a
  warm workspace (no I/O outside what's already cached in-process).
  A criterion benchmark asserts this — the verb is meant to feel
  instant, and a slow preamble would block every turn
- {#c-project-summary-surface} when
  [REQ:codon/project-knowledge-base](spec:REQ:codon/project-knowledge-base)
  is enabled and a directory or project summary exists for the
  focused pane's cwd (most-specific match wins; falls back to the
  project summary at the workspace root), the summary surfaces
  into the preamble's memory section. Counts against the same
  25%-of-budget cap as `c-memories-budgeted` and never displaces
  the core sections. When project-kb is disabled the section is
  silently omitted
:::

## Out of scope

- A full retrieval-augmented context layer. Memory surfacing here
  is intentionally cheap (pinned + keyword filter); semantic search
  over project sources is out of scope and would belong to a
  separate REQ.
- Per-flow preamble customization. Every flow gets the same
  preamble. If a flow needs extra context, it goes in the user
  message, not the preamble.
- Compressing or summarizing the preamble at the model side. The
  byte budget is enforced at assembly time; nothing post-processes
  the result.
