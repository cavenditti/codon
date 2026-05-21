---
id: TOPIC:topics/phase-22
type: topic
status: draft
version: 0.1.0
summary: >
  Deep agentic integration. Codon already has a first-class agent pane
  and three selection-seeded verbs (`AgentExplain`, `AgentSummarize`,
  `AgentRefactor`), but every agent interaction today starts from a
  selection and ends in the agent's own message editor. Phase 22 turns
  the agent into a global, contextual collaborator: a single
  keybinding reads the focused pane's kind + state, gives the agent a
  small deterministic context preamble plus a set of pane-inspection
  tools, and surfaces the agent's suggestion (a shell command, an
  action, or a plain response) in a confirm-before-applying overlay.
  Every flow routes through one shared harness with cancellation,
  tracing, and searchable cross-session memories.
owners: [carlo]
---

# Phase 22 — Deep agentic integration

## Why now

The phase-3 cross-pane verbs proved the seeding pattern works: from
any pane a user can fire one keystroke and land in the agent with the
relevant selection. As the agent has become a daily-driver tool,
three structural gaps have emerged:

1. **No "ask about this pane" entry point.** The selection-seeded
   verbs require a selection. The agent panel itself is the only way
   to ask a free-form question, and reaching it from a terminal — the
   pane kind that benefits most from an LLM ("what flag does
   `pg_dump` take for…") — is a context switch out of the work.
2. **The agent has no first-class view of the rest of the
   workspace.** Today it only sees what the user pasted in. No tool
   lets the model grep the focused terminal's scrollback, list
   visible panes, or pull the FM's current directory listing. So
   even when the user *does* paste context, the model can't follow
   up.
3. **No shared substrate.** The three existing verbs each construct
   their own prompt prefix. There is no fixed "what does the agent
   always know about the session" preamble, no shared tool registry,
   and no shared memory store. Adding a fourth or fifth flow would
   continue the per-feature pattern.

Phase 22 closes those gaps with a single global verb backed by a
tools layer, a deterministic preamble, a memory store, and one
harness shared by every agent interaction in codon.

## Scope

In scope:

- A global contextual-suggest verb (`codon_agent::ContextualSuggest`,
  proposed bind `prefix '`) that opens an NL input modal whose
  follow-up rendering depends on the focused pane kind.
- A pane-tools layer the agent calls during a turn: grep/read of the
  current pane, grep/read of named other panes, list-panes,
  `suggest_action`, `suggest_response`.
- A deterministic, byte-budgeted unconditional preamble injected
  ahead of every agent turn (contextual-suggest *and* the existing
  cross-pane verbs).
- A workspace-scoped memory store with search exposed as a tool plus
  a picker for direct user inspection.
- A unified agent harness shared by every agent flow in codon.
  Evaluation of [forge](https://github.com/antoinezambelli/forge) as
  the harness library is a clause; building a thin in-house loop is
  the documented fallback if forge does not fit codon's tool surface.

Out of scope (deferred or wontdo):

- **Auto-executing suggested shell commands.** Every command flows
  through a confirm-overlay; the user accepts (sends to PTY), edits,
  or dismisses. Hard non-goal for phase 22 — the keyboard-first
  model demands the user owns the final keystroke.
- **Voice input.** Already declared out-of-scope in
  `REQ:codon/discoverability`.
- **Multi-agent / agent-team orchestration.** A single agent
  responds; phase-22 ships no fan-out.
- **Cross-workspace memory.** Memories are scoped to the current
  workspace's root. A global scope can be added later when there is
  a real use case.

## Refining requirements

- [REQ:codon/agent-contextual-suggest](spec:REQ:codon/agent-contextual-suggest)
  — the global pane-aware NL entry point and its per-pane rendering
  rules.
- [REQ:codon/agent-pane-tools](spec:REQ:codon/agent-pane-tools) — the
  tool surface the agent calls during a turn.
- [REQ:codon/agent-context-preamble](spec:REQ:codon/agent-context-preamble)
  — the deterministic, byte-budgeted prefix every interaction
  carries.
- [REQ:codon/agent-shared-memory](spec:REQ:codon/agent-shared-memory)
  — workspace-scoped memory store + picker + search tool.
- [REQ:codon/agent-harness](spec:REQ:codon/agent-harness) — the
  shared loop (forge, or in-house) that drives every agent flow.
- [REQ:codon/command-history](spec:REQ:codon/command-history) —
  indexed, AI-summarized shell command history sourced from OSC 133
  boundaries. Searchable as an agent tool + picker.
- [REQ:codon/project-knowledge-base](spec:REQ:codon/project-knowledge-base)
  — per-directory and per-project rollup summaries built from
  command-history + memories; surfaces into the preamble.
- [REQ:codon/secret-redaction-pipeline](spec:REQ:codon/secret-redaction-pipeline)
  — staged redactor (pattern + entropy + model-based NER) every
  LLM-bound byte passes through; fail-closed; reused by all
  LLM-producing surfaces.
- [REQ:codon/fish-shell-integration](spec:REQ:codon/fish-shell-integration)
  — fish plugin + per-workspace Unix-socket RPC. `codon do
  <action>` dispatches any codon action from the shell; `#@`
  syntax + `Ctrl-G` trigger runs the agent against the buffer
  inline. Same harness, same redaction pipeline. Bash and zsh
  are deferred.

## Open questions captured for phase planning

- Does forge's API shape compose with codon's "tools that read GPUI
  entities" surface? The harness REQ has an evaluation clause whose
  TASK delivers a one-page memo and a sample wire-up before any
  irreversible adoption.
- Does the contextual-suggest overlay live as a `codon-pickers`
  ModalScaffold or as a dedicated pane kind? Default position: a
  modal (reuses focus + dismiss plumbing). Re-open if a pane is
  actually needed.
- For terminals, where does the suggested command land? Default
  position: prefilled at the PTY cursor in Insert mode, *not* sent;
  the user types Enter to execute. No history-line trickery — just
  a prefill the user can edit.
