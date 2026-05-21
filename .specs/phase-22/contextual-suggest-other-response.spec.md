---
id: TASK:phase-22/contextual-suggest-other-response
type: task
status: accepted
version: 0.1.0
summary: >
  Render `SuggestResponse` results in any pane: read-only text body,
  `esc` to dismiss, `y` to yank to clipboard. The fallback shape for
  agent/outline/git/debug/peek/welcome panes and the simple Q&A path
  in every other pane.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-contextual-suggest#c-other-response
---

# Other-pane shape: SuggestResponse renderer

## Plan

- Extend `contextual_overlay.rs` with a `ResponseView` rendered
  when the harness returns
  `TurnOutcome::Suggestion(SuggestResponse { text })`.
- Layout: a scrollable read-only text block (reuse Zed's
  `Editor::read_only(true)` or a `Markdown` view if the response
  is markdown-tagged), word-wrapped to the overlay's width. Footer:
  `esc` dismiss · `y` copy.
- `y` writes `text` to the system clipboard via the existing
  `cx.write_to_clipboard` path, then shows a quick toast.
- The view honours codon's font-zoom (`cmd-=` / `cmd--`).
- This renderer is shape-only; the *router* (in pane-tools-suggest-
  shapes + agent-pane-tools spec) is what decides
  `SuggestResponse` is legal for agent/outline/git/debug/peek/
  welcome and also legal as a fallback in any other pane.

## Acceptance

- Synthetic turn returning `SuggestResponse { text: "..." }` from
  any pane opens the read-only view.
- `y` copies the body to the clipboard (test via
  `cx.read_from_clipboard` round-trip).
- Esc closes the view.
- Scroll behaviour matches the cheatsheet modal (j/k or arrow keys
  if the body exceeds the viewport).
