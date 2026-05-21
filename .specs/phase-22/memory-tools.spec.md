---
id: TASK:phase-22/memory-tools
type: task
status: accepted
version: 0.1.0
summary: >
  Implement the three memory tools (`search_memories`, `list_memories`,
  `remember`). `remember` queues a write that only commits after the
  user accepts a confirm-overlay; the tool's return value tells the
  agent whether the user accepted.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shared-memory#c-tool-search
  - REQ:codon/agent-shared-memory#c-tool-list
  - REQ:codon/agent-shared-memory#c-tool-remember
  - REQ:codon/agent-shared-memory#c-no-silent-write
aspects: [search-tool, list-tool, remember-tool, confirm-before-write]
---

# Memory tools (search / list / remember)

## Plan

- New module `crates/codon-agent/src/tools/memory.rs`.
- `search_memories { query, tags?, max_hits? }` → `Vec<MemoryEntry>`:
  substring match over title + body + tags (case-insensitive).
  Returns full bodies subject to the harness per-call byte budget;
  larger results are truncated with `truncated: true`.
- `list_memories { pinned_only? }` → `Vec<MemoryStub>` where stubs
  have title + tags + pinned + created. No bodies, cheap to call.
- `remember { title, body, tags?, pinned? }`:
  1. Validate the body against the secret-redaction list (delegated
     to sibling task `memory-secret-redaction`). On match → return
     `redaction_required` with the offending pattern; do NOT show
     the user the overlay.
  2. Validate the body length (≤ 4 KiB). On too-large → return
     `body_too_large`.
  3. Open a `MemoryConfirmOverlay` showing the proposed title +
     body. Footer: `enter` save · `e` edit · `esc` reject.
  4. On `enter`: call `MemoryStore::write(entry)`. Return
     `accepted { id }` to the agent.
  5. On `e`: open the inline editor (multi-line, same pattern as
     terminal-shape edit). A second `enter` saves the edited
     version.
  6. On `esc`: return `rejected` to the agent. No file is written.
- The harness MUST treat `remember` as a tool that *might* block on
  user input. Cancellation while the overlay is open returns
  `rejected`.

## Acceptance

- A `search_memories { query: "DB" }` over a fixture with two
  matching memories returns both.
- `remember { title: "...", body: "..." }` opens the overlay;
  pressing `enter` writes a file under the store directory; the
  tool returns `accepted` with the id.
- `remember { ... body: "AWS_SECRET=abc...xyz" }` returns
  `redaction_required` and never opens the overlay.
- `esc` on the overlay returns `rejected` and the directory has no
  new file.
- `cargo test -p codon-agent` passes.
