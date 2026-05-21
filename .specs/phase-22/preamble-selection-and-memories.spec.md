---
id: TASK:phase-22/preamble-selection-and-memories
type: task
status: accepted
version: 0.1.0
summary: >
  Emit the selection-summary section when a `SelectionSource` is
  present, and surface pinned + keyword-matched memories into the
  preamble within the 25%-of-budget memory cap.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-context-preamble#c-selection-summary
  - REQ:codon/agent-context-preamble#c-memories-budgeted
aspects: [selection-summary, memory-surface]
---

# Preamble: selection summary + memory surfacing

## Plan

- Selection summary:
  - Resolve via `codon_pane_bridge::SelectionSource` (the existing
    Phase 1 abstraction).
  - Emit one line: `selection: <kind> (<n> bytes, <m> lines)`.
    `<kind>` is the `SelectionSource` variant tag. Full selection
    text is NOT in the preamble — flows that need it inject it into
    the user message body.
  - Section is omitted entirely when no selection is present.
- Memory surfacing:
  - Pull from
    [REQ:codon/agent-shared-memory](spec:REQ:codon/agent-shared-memory)
    via `codon_memory::for_preamble(query, budget) ->
    Vec<MemoryEntry>`.
  - Inputs: the user's question (if available — for
    contextual-suggest, yes; for cross-pane verbs, the verb name +
    selection kind), and the available budget (= 25% of the
    `byte_budget`, rounded down to whole entries).
  - Order: pinned first, then keyword-matched by descending overlap
    count, then by `created` ascending.
  - Each entry renders as `- [<title>] <body-first-line>` truncated
    to a one-line summary inside the budget.
- The whole memories block is dropped first when the overall
  preamble budget is tight — coordinate with sibling task
  `preamble-budget-determinism`.

## Acceptance

- Unit test: a preamble built with an active editor selection
  shows the selection-summary line; without selection it's absent.
- Unit test: with 5 pinned memories whose combined width exceeds
  the 25% slice, only the first N fit (ordered as documented), the
  rest drop.
- Property test: same fixture inputs → byte-identical memory block.
- `cargo test -p codon-agent` passes.
