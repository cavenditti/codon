---
id: TASK:phase-5/command-palette-error-surface
type: task
status: accepted
version: 0.0.1
summary: >
  Stop silently swallowing completer errors in the command palette —
  today the modal does `.log_err().unwrap_or_default()` and the user
  sees an empty result list with no explanation when a completer
  fails.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-palette
  - REQ:codon/code-quality#c-error-visibility
aspects: [completer-error-row, palette-error-visibility]
---

# Surface completer errors in the palette

## What ships

[`crates/codon-command-palette/src/modal.rs:522`](spec:src:crates/codon-command-palette/src/modal.rs)
runs each completer via:

```rust
let items = completer.complete(query, cx).await
    .log_err()
    .unwrap_or_default();
```

That pattern is correct for diagnostic / background plumbing where
nobody is watching. It is wrong for the command palette, where the
user is actively typing and expecting either matches or a reason
they're not seeing matches. Today, if `FilePathCompleter` hits a
permission error or `SearchCompleter` fails to spawn ripgrep, the
palette shows "no results" — indistinguishable from "no matches."

## What changes

- `complete()` returns `Result<Vec<CompletionItem>>`. Keep the
  `Result` instead of collapsing it.
- The palette's result-rendering branch handles three cases:
  1. `Ok(items)` with len > 0 → render rows (today's behaviour).
  2. `Ok(items)` with len == 0 → render the existing "no matches"
     empty state.
  3. `Err(e)` → render a single muted-red row showing the error's
     `Display` form, prefixed with the completer name (e.g.,
     `file_path: Permission denied (os error 13)`). The row is not
     selectable.
- The error row replaces the result list for that completer only —
  the description pane still works, the user can backspace and
  recover.
- The original `.log_err()` call should remain (or move to the
  background spawn) — we still want the error in the log, in
  addition to surfacing it to the user.

## File anchors

- [`crates/codon-command-palette/src/modal.rs`](spec:src:crates/codon-command-palette/src/modal.rs)
  — the rendering site at line 522 and wherever the result list
  layout currently lives.
- [`crates/codon-command-palette/src/completer.rs`](spec:src:crates/codon-command-palette/src/completer.rs)
  — `Completer::complete` already returns
  `Task<Result<Vec<CompletionItem>>>`, so the trait surface does not
  change.

## Acceptance

- Force a completer error (e.g., point `FilePathCompleter` at a
  directory you've `chmod 000`'d) and the palette renders a visible
  error row, not an empty list.
- The error row is unselectable — pressing Enter on it does nothing.
- The successful path is byte-identical to today's behaviour
  (no extra row when the completer succeeds).
- Tests cover the three rendering branches with a fake completer
  that returns `Ok(vec![])`, `Ok(vec![item])`, and
  `Err(anyhow::anyhow!("boom"))`.

## Out of scope

- A unified toast system across all codon panes — that's
  REQ-level work, not this task.
- Per-completer retry UX — out of scope; user can re-type to
  re-trigger.

Effort: small. ~60 LOC of rendering + ~80 LOC of tests.
