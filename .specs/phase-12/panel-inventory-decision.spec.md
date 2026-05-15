---
id: TASK:phase-12/panel-inventory-decision
type: task
status: accepted
version: 0.0.1
summary: >
  Lock in the per-panel verdict (convert / peek-only / drop /
  already-replaced) for each of the seven Zed Panel impls.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-inventory
---

# Panel inventory and per-panel verdict

## The seven panels

| Panel | Crate | File |
|---|---|---|
| `AgentPanel` | `agent_ui` | `vendor/zed/crates/agent_ui/src/agent_panel.rs:2480` |
| `ProjectPanel` | `project_panel` | `vendor/zed/crates/project_panel/src/project_panel.rs:7274` |
| `OutlinePanel` | `outline_panel` | `vendor/zed/crates/outline_panel/src/outline_panel.rs:4888` |
| `TerminalPanel` | `terminal_view` | `vendor/zed/crates/terminal_view/src/terminal_panel.rs:1538` |
| `GitPanel` | `git_ui` | `vendor/zed/crates/git_ui/src/git_panel.rs:6243` |
| `DebugPanel` | `debugger_ui` | `vendor/zed/crates/debugger_ui/src/debugger_panel.rs:1538` |
| `CollabPanel` | `collab_ui` | `vendor/zed/crates/collab_ui/src/collab_panel.rs:3832` |

## Verdicts

### Convert — adapter-hosted as a pane (default open) + peek (modifier)

- **AgentPanel**. The cross-pane verbs work
  ([REQ:codon/agent-pane](spec:REQ:codon/agent-pane)) but the agent
  is structurally still a side rail. Pane placement lets users keep
  the thread open in a split while editing. Peek-side preference:
  `right`.
- **GitPanel**. Today modal-integrated as a dock
  ([TASK:phase-4/git-panel-modal-integration](spec:TASK:phase-4/git-panel-modal-integration));
  the dispatch-context patches stay. Pane placement is the new
  default; peek (preference: `left`) preserves the
  staging-while-committing workflow.
- **OutlinePanel**. Currently unused in codon. Convert and wire a
  default cmd-k binding so symbol outline is reachable without
  re-implementing it. Peek-side preference: `left`.
- **DebugPanel**. Convert. Debugger work isn't a codon priority yet,
  but the adapter makes it trivial to expose. Peek-side preference:
  `bottom` (mirrors typical debugger placement).

### Already-replaced — no codon entry point needed

- **ProjectPanel**. Superseded by codon's `file-manager` crate.
  Keep upstream code intact for the diff, but ship no codon action
  that opens it. `cmd-k <chord>` for "file tree" maps to
  `file-manager`'s pane open. Recorded here so the audit doesn't
  reopen it.

### Drop — no codon entry point, no peek

- **TerminalPanel**. The *panel* is a host for terminal *tabs*;
  codon's terminals are already panes via `codon-session`. There is
  no codon-side use for the panel form. Recorded as dropped.
- **CollabPanel**. Single-user fork; channels / calls / chat are out
  of scope.

## Approach

This task is editorial — it locks the decisions into the spec graph
so subsequent migration tasks have a clear scope. Output:

1. Update `REQ:codon/panes-from-panels#c-inventory` if any verdict
   shifts during the prototype of the adapter task.
2. Each *convert* verdict spawns its own migration task
   (`agent-panel-migration`, `git-panel-migration`,
   `outline-panel-migration`, `debug-panel-migration`).
3. Each *drop* / *already-replaced* verdict is recorded here only
   — no follow-up task. If a future user complains, the verdict
   gets revisited via a new TASK that flips it to *convert*.

## Non-goals

No code changes. No keymap edits. The cmd-k chord assignments live
in `panel-pane-keymap-surface`, not here.
