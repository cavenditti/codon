---
id: TASK:phase-5/cheatsheet-contextual-section
type: task
status: accepted
version: 0.0.1
summary: >
  Cheatsheet (`cmd-k F1`) gains a top "This pane" section listing
  bindings reachable from the current dispatch chain. Existing global
  section follows below.
owners: [carlo]
progress: done
refines:
  - REQ:codon/modal-shell#c-cheatsheet-contextual
---

# Contextual cheatsheet section

## What ships

The cheatsheet modal at `crates/codon-keymap/src/cheatsheet_modal.rs`
already renders every reachable binding via
`window.possible_bindings_for_input(&[])`, grouped by chord prefix.
Two additions:

- **Capture the focus chain at open time.** When the action handler
  for `ShowKeymap` fires, the focused element's dispatch chain
  (e.g. `GitStatus`, `Terminal && pane_mode == normal`,
  `FileManager && pane_mode == insert`) is captured into the modal.
- **Split rows into two buckets.** Bindings whose context predicate
  matches the captured chain go into a new top section labelled
  *"This pane"*; everything else (no context, or context that doesn't
  match the chain) goes into the existing *"Global"* section below.

If the active pane has no pane-specific bindings the top section
collapses to a single muted line ("No pane-specific bindings") rather
than disappearing, so the layout doesn't shift between panes.

## Why stacked over tabbed

Stacked sections show both groups in one pass — no extra keystroke
to reveal the other half — and a sane fallback when the pane is
context-less. A tabbed `Current` / `Global` view with `Tab` to switch
was the alternative; rejected because it doubles UI plumbing (focus
management, tab indicator, switch binding) for marginal information
gain when both groups usually fit on one screen.

## Files to modify

- `crates/codon-keymap/src/cheatsheet_modal.rs` —
  `KeybindingsCheatsheetModal::new` takes the captured key-context
  chain; `collect_bindings` splits rows; `render` adds the *This
  pane* header above the existing groups.
- Possibly a new helper in the same file for context-predicate
  matching. `KeyContext::predicate_eval` (or similar) exists in
  `gpui::KeyContext` — re-use rather than re-parse.

Effort: small. ~100 LOC plus header rendering.
