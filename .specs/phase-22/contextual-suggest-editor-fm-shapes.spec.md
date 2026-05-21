---
id: TASK:phase-22/contextual-suggest-editor-fm-shapes
type: task
status: accepted
version: 0.1.0
summary: >
  Render `SuggestAction` results in editor and file-manager panes: a
  preview of the named action + payload, Enter to dispatch, Esc to
  dismiss. Action name MUST resolve through Zed's global action
  registry; an unresolved name fails closed.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-contextual-suggest#c-editor-action
  - REQ:codon/agent-contextual-suggest#c-fm-action
aspects: [editor-action-preview, fm-action-preview]
---

# Editor + FM shape: SuggestAction renderer

## Plan

- Extend `contextual_overlay.rs` with an `ActionConfirm` view
  rendered when the harness returns
  `TurnOutcome::Suggestion(SuggestAction { action_name, payload, why })`.
- Resolve `action_name` against the global action registry. On
  failure, surface a structured trace entry and render an error
  toast saying "agent suggested an unknown action: <name>". Do not
  dispatch.
- Preview layout: the action's human-readable display name (resolved
  via the registry), a one-line summary of the payload, then the
  rationale. Footer: `enter` dispatch · `esc` dismiss.
- Enter: dispatch the action against the *previously-focused* pane
  (recorded on overlay open). Use `cx.dispatch_action_in_pane` or
  the equivalent existing path so the action sees the right focus
  chain — not the overlay itself.
- For editor panes the payload may include text edits (insert /
  replace / delete) encoded as a small JSON schema. Add a
  `EditorActionPayload` decoder under `crates/codon-agent/src/`
  shared with the harness's tool schemas.
- For FM panes the payload may include a target path (an entry
  under the FM's `current_dir`) or a marked-rows verb. Reject paths
  outside the FM's current directory.
- Both renderers MUST refuse a `SuggestAction` whose `action_name`
  begins with `codon_agent::` to avoid recursion (the agent
  re-suggesting another agent verb). Surfaced as a trace entry; UI
  shows nothing.

## Acceptance

- Synthetic harness turn returning
  `SuggestAction { action_name: "editor::Format" }` against an
  editor → Enter formats the buffer.
- Synthetic harness turn against the FM with
  `SuggestAction { action_name: "file_manager::ToggleHidden" }` →
  Enter toggles hidden visibility.
- Unknown action name → toast + harness trace records the failure;
  no dispatch.
- `codon_agent::*` action names → silently refused with trace entry.
- `cargo test -p codon-agent` passes.
