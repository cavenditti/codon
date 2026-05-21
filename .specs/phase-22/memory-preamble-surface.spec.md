---
id: TASK:phase-22/memory-preamble-surface
type: task
status: accepted
version: 0.1.0
summary: >
  Expose `codon_memory::for_preamble(query, budget) ->
  Vec<MemoryEntry>` — pinned first, then keyword-overlap matches
  against the user's question, ordered deterministically. Enforces
  the workspace-scope rule.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shared-memory#c-preamble-surface
  - REQ:codon/agent-shared-memory#c-workspace-scope
aspects: [for-preamble-fn, workspace-scope]
---

# Memory surfacing for the preamble

## Plan

- Add `pub fn for_preamble(query: Option<&str>, byte_budget: usize)
  -> Vec<MemoryEntry>` on `MemoryStore`.
- Selection algorithm:
  1. Start with all pinned entries (preserves pinned-first ordering).
  2. If a query is provided, compute case-insensitive overlap
     between query tokens and (title + tags + body) tokens. Sort
     non-pinned matches by descending overlap count.
  3. Greedily pack entries into the budget — each entry contributes
     its rendered preamble line width.
  4. Ties broken by `created ASC` to keep the result deterministic.
- Workspace scope: the function operates only on the store opened
  for the active workspace. There is no API path that crosses
  workspaces; this is enforced by `MemoryStore` taking the
  fingerprint at construction time.
- Returns an empty Vec when no matches and no pinned — caller
  drops the section.

## Acceptance

- Unit test with 3 pinned + 5 unpinned memories, query "deploy":
  pinned first, then unpinned in descending overlap order. Result
  deterministic across runs (property test).
- Budget cap test: a small budget yields a prefix of the ordered
  list, never reordered or shuffled.
- Workspace isolation test: opening two `MemoryStore`s for
  different fingerprints returns disjoint sets.
- `cargo test -p codon-memory` passes.
