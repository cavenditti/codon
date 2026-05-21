---
id: REQ:codon/agent-contextual-suggest
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  A single global verb opens an NL prompt whose downstream rendering
  is determined by the focused pane's kind and state. The agent
  responds with a shell command (terminal), a structured action
  suggestion (editor / FM / git / outline), or a free-text answer
  (anywhere) — surfaced in a confirm-before-applying overlay. Nothing
  the agent suggests is executed without an explicit user keystroke.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Agent contextual suggest

## Context

Today the agent is reachable two ways: the agent pane itself
(`prefix a`), and three selection-seeded verbs (`prefix a e/s/r`)
that require a non-empty selection. From a terminal — the pane kind
where "what's the command for…" comes up most often — neither path
fits: there is rarely a selection, and the panel context-switches
away from the work.

Contextual-suggest closes the gap. It is one verb. It opens a small
NL input field. It hands the agent (a) the user's question and (b)
the standard context preamble from
[REQ:codon/agent-context-preamble](spec:REQ:codon/agent-context-preamble),
which already encodes the focused pane's kind + state. The model
chooses between three response shapes via the tools in
[REQ:codon/agent-pane-tools](spec:REQ:codon/agent-pane-tools):
`suggest_command` (terminal only), `suggest_action` (any pane), or
`suggest_response` (any pane). The host renders the result in a
focus-trapped overlay; the user accepts, edits, or dismisses by
keyboard.

Two invariants make the feature safe:

1. **No auto-execute.** A suggested shell command is *prefilled* at
   the terminal's PTY cursor in Insert mode. The user owns the Enter
   key. A suggested action is *previewed* in the overlay; the user
   confirms (the host dispatches the action) or dismisses. A response
   is rendered inline; nothing else happens.
2. **The pane router decides what shapes are legal.** In a terminal
   the agent may use `suggest_command` or `suggest_response`; in an
   editor `suggest_action` or `suggest_response`; in the FM
   `suggest_action` or `suggest_response`; in agent / outline / git /
   debug, only `suggest_response`. The router enforces this on the
   tool surface so a misrouted shape is rejected before it reaches
   the UI.

:::{requirement id="agent-contextual-suggest" level="MUST"}
The system MUST provide:

- {#c-global-action} a workspace-registered action
  `codon_agent::ContextualSuggest` that fires regardless of the
  focused pane kind, bound by default to `prefix '` in the embedded
  codon TOML (chord is configurable via `[keymap] prefix = "..."` and
  rebinding through user `codon.toml` as any other action)
- {#c-input-overlay} on activation, an NL input modal opens (built on
  `codon-pickers::ModalScaffold`) with a prompt label that reflects
  the focused pane kind ("ask about this terminal", "ask about this
  buffer", "ask about this directory", "ask"). Enter submits; Escape
  cancels with no agent call
- {#c-pane-router} a pane-router resolves the focused pane via
  `codon_pane_bridge::CodonModeTracker` + the pane's
  `PaneModeBridge::kind()` and selects the legal response shapes for
  that kind. The router is a single function so adding a new pane
  kind is a one-line change
- {#c-terminal-command} when the focused pane is a terminal, the
  legal shapes are `suggest_command` and `suggest_response`. A
  `suggest_command` result is rendered in a confirm-overlay with the
  command pre-formatted, a one-line explanation, and three keyboard
  options: Enter (prefill at PTY cursor, return to terminal Insert
  mode without sending), `e` (edit the command in-overlay before
  prefill), Escape (dismiss)
- {#c-editor-action} when the focused pane is an editor, the legal
  shapes are `suggest_action` and `suggest_response`. A
  `suggest_action` result names a registered codon action plus
  optional payload; the overlay shows the action's display name and
  any text edits or motions it implies. Enter confirms (the host
  dispatches the action against the editor); Escape dismisses
- {#c-fm-action} when the focused pane is the file manager, legal
  shapes are `suggest_action` and `suggest_response`. Actions cover
  `file_manager::*` verbs over the marked set (or the row under
  cursor when nothing is marked). Same confirm/edit/dismiss
  semantics as the editor path
- {#c-other-response} for every other pane kind (agent, outline,
  git, debug, peek, welcome), only `suggest_response` is legal. The
  overlay renders the response as read-only text with Esc-to-dismiss
  and `y` to yank into the clipboard
- {#c-no-auto-execute} no shape — command, action, or response — is
  ever applied without an explicit confirming keystroke. This is a
  hard invariant. A future "auto-apply trusted suggestions" toggle
  is explicitly deferred and MUST NOT ship in phase 22
- {#c-cancellation} an in-flight agent turn is cancellable via
  Escape (in addition to dismissing the overlay's render). The
  cancellation routes through the shared harness — see
  [REQ:codon/agent-harness](spec:REQ:codon/agent-harness)#c-cancellation
- {#c-mode-bridge} while the overlay is open, the pane-mode tracker
  reports `PaneMode::Command` (consistent with other codon modals).
  The status bar mode pill follows
- {#c-no-selection-required} the verb works with no selection. When
  a selection is present, it is included in the preamble (handled
  by REQ:codon/agent-context-preamble); the verb does not require
  one or special-case its presence
- {#c-cheatsheet} the binding surfaces in `cmd-k F1` under the
  "Global" section with a "this pane" qualifier in the description,
  so the user can discover that the same chord behaves differently
  in different panes
:::

## Out of scope

- Auto-executing shell commands. Explicit non-goal.
- Multi-turn conversations from within the overlay. If the user
  wants to follow up, they dismiss and open the agent pane
  (`prefix a`) where the harness-shared history is already present.
- Voice input. Out of scope per `REQ:codon/discoverability`.
