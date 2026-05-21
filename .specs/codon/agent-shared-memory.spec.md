---
id: REQ:codon/agent-shared-memory
type: requirement
status: draft
version: 0.1.0
level: SHOULD
summary: >
  A workspace-scoped, searchable memory store the agent can read,
  search, and (with explicit user confirmation) append to. Memories
  are short titled notes — facts about the project, recurring
  preferences, decisions worth not re-discovering — surfaced into the
  preamble when small and pinned, queryable via a tool, and editable
  by the user via a picker. Per-workspace; per-workspace persisted
  to disk under codon's config directory; never cross-workspace.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Agent shared memory

## Context

Every agent turn currently starts from a blank slate. Facts the
user has already taught the agent (`we use pnpm not npm`, `the
production DB is in eu-west-1`, `agent verbs land in `crates/codon-
agent/src/actions.rs`) get re-discovered turn after turn, or get
hard-coded into the user's question. Both are friction.

Shared memory is codon's answer: a small, persisted, searchable
store of titled notes the agent can read, search, and append to.
The Claude Code memory system the user already runs in this
session is the inspiration — pinned facts live in an index, agent
calls have a tool to grep them, the user can curate the list
through a picker.

Three design constraints:

1. **Workspace-scoped, not global.** Memories live under
   `~/.config/codon/memories/<workspace-fingerprint>/`. A
   fingerprint is derived from the workspace root path (stable as
   long as the path is stable). Memories never leak across
   workspaces. A future global scope can be added if a real use
   case emerges — phase 22 ships per-workspace only.
2. **Append is explicit.** The agent calls `remember(title, body,
   tags?, pinned?)`, but the host surfaces a confirm-overlay
   ("agent wants to remember: …") before writing to disk. The
   user accepts, edits, or rejects. Identical confirm-and-apply
   model as the suggestion overlays from agent-contextual-suggest.
3. **Read and search are cheap.** The store is a directory of
   markdown files with YAML frontmatter; the index is rebuilt from
   the directory on workspace open (no separate DB to keep in
   sync). Search is substring over title+body+tags; phase 22 does
   not ship a vector store. Top-N most-pinned and keyword-matched
   memories surface into the preamble (see
   [REQ:codon/agent-context-preamble](spec:REQ:codon/agent-context-preamble)#c-memories-budgeted).

:::{requirement id="agent-shared-memory" level="SHOULD"}
The system SHOULD provide:

- {#c-store-layout} a per-workspace store at
  `~/.config/codon/memories/<fingerprint>/` where each memory is a
  single `.md` file with frontmatter (`title`, `created`, `tags`,
  `pinned`) and a markdown body ≤ 4 KiB
- {#c-fingerprint} a workspace fingerprint derived deterministically
  from the canonicalised workspace root path. Renaming the
  directory is the same as starting fresh — that is acceptable
  behaviour and surfaces in the picker (the "no memories" empty
  state is enough)
- {#c-index-on-open} the workspace's memory index is built on
  workspace open by listing the store directory. No separate cache
  file; the FS *is* the index. Reads after open are in-memory
- {#c-tool-search} a `search_memories(query, tags?, max_hits?)`
  tool exposed via the agent harness — substring match over
  title + body + tags. Returns the matching memories with full
  body (subject to the harness byte budget per call)
- {#c-tool-list} a `list_memories(pinned_only?)` tool returns
  titles + tags + pinned flags (no body) for cheap enumeration
- {#c-tool-remember} a `remember(title, body, tags?, pinned?)` tool
  — the agent calls it, the host shows a confirm-overlay
  ("Remember: <title>" with the body preview), and only on user
  confirmation does the file land on disk
- {#c-no-silent-write} the harness MUST NOT write to the store
  without user confirmation. `remember` returning success to the
  agent only means the proposal was queued; the file write happens
  after the confirm-overlay returns accept. The model is told via
  the tool's return value whether the user accepted or rejected
- {#c-picker} a `codon_memory::MemoryPicker` modal (built on
  `codon-pickers::ModalScaffold`) — bound by default to
  `prefix m` — lists every memory, fuzzy-searchable by title/tags.
  Enter opens the file in an editor pane; `p` toggles pinned;
  `dd` deletes (with confirm); `c` creates a new memory by hand
- {#c-preamble-surface} pinned memories surface into the preamble
  per REQ:codon/agent-context-preamble#c-memories-budgeted.
  Non-pinned memories surface only when a cheap keyword overlap
  with the user's question exists. Phase 22 does not ship semantic
  ranking
- {#c-workspace-scope} memory tools and the picker only ever
  expose the current workspace's store. Cross-workspace recall
  is explicitly out of scope; even a global pinned set is
  deferred
- {#c-shape-budget} a single memory body is capped at 4 KiB; the
  store as a whole is uncapped but the picker shows a soft warning
  when total store size exceeds 256 KiB so the user can prune
- {#c-no-secrets} memories MUST NOT be written if the body matches
  the same secret-pattern list from
  REQ:codon/agent-context-preamble#c-no-secrets. The `remember`
  confirm-overlay surfaces the redaction reason; the user can
  edit the body in-place to remove the match and reconfirm
- {#c-export-import} a `codon_memory::Export` action writes the
  store to a tarball under `$workspace_root/.codon/memories.tar`
  (gitignored by default); `codon_memory::Import` reads one back.
  Lets a user move memories between machines without a central
  service. The corresponding TASK is OPTIONAL for phase-22 ship
:::

## Out of scope

- Vector / semantic search. Substring match is sufficient at the
  scales codon's memory store reaches; vectorising adds a heavy
  dependency for marginal benefit at < 1 KB-per-note scale.
- Cross-workspace memories. Defer until a real use case arrives.
- Sharing memories across users / machines via a central service.
  The export/import escape hatch in `c-export-import` is enough
  for phase 22.
- A "memory garbage collector" that drops stale entries. The
  picker lets the user prune; automated dropping is too easy to
  get wrong.
