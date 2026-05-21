---
id: TASK:phase-22/pane-tools-read-current
type: task
status: accepted
version: 0.1.0
summary: >
  Implement `grep_current_pane` and `read_current_pane` agent tools
  against the focused pane's `PaneInspect` impl. Honor the per-call
  byte budget; surface clear errors when the pane kind has no
  content.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-pane-tools#c-tool-grep-current
  - REQ:codon/agent-pane-tools#c-tool-read-current
aspects: [grep-current, read-current]
---

# Tools: read/grep the focused pane

## Plan

- New module `crates/codon-agent/src/tools/pane_read.rs` with two
  tools:
  - `grep_current_pane { pattern: String, max_hits: Option<usize> }`
    → `Vec<SearchHit>`.
  - `read_current_pane { scrollback: Option<bool>, offset:
    Option<usize> }` → `PaneSlice`.
- Tools resolve the focused entity through
  `CodonModeTracker::focused()` and call the corresponding
  `PaneInspect` method.
- Tool error shapes (returned to the model, not panics):
  - `pane_kind_unsupported`: pane returned the trait's default
    no-op (e.g. user invoked over the welcome page).
  - `pattern_invalid`: regex compilation failed (for the regex
    branch — phase 22 may ship literal-only and add regex later).
  - `byte_budget_exceeded`: pattern matched but the result hit the
    per-call cap before completing. The partial result is returned
    with `truncated: true`.
- Tools register against the harness through the trait surface from
  `harness-api` (sibling task). No direct GPUI hooks here — the
  registry takes a closure that calls the tool.

## Acceptance

- Synthetic harness turn that calls `grep_current_pane { pattern:
  "ERROR" }` against a terminal containing two ERROR lines returns
  exactly two `SearchHit`s with surrounding snippets.
- `read_current_pane { scrollback: true, offset: 0 }` against the
  same terminal returns the visible region first, then accepts the
  returned `next_offset` to read the next chunk deterministically.
- A call from the welcome page returns `pane_kind_unsupported`.
- `cargo test -p codon-agent` passes.
