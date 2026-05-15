---
id: TOPIC:topics/phase-12
type: topic
status: draft
version: 0.0.1
summary: >
  Panes-from-panels: every Zed dock-hosted Panel becomes a first-class
  pane in the workspace tree, with an opt-in transient "peek" mode that
  re-uses the dock surface for on-demand sidebar viewing.
owners: [carlo]
---

# Phase 12 — Panes from panels

Codon's core invariant is *every surface is a pane in the workspace
tree*. Today seven Zed views still violate that invariant by living in
the left / right / bottom docks: `AgentPanel`, `ProjectPanel`,
`OutlinePanel`, `TerminalPanel`, `GitPanel`, `DebugPanel`,
`CollabPanel`. The keyboard surface around them has been patched into
codon's modal model piecemeal (see
[TASK:phase-4/git-panel-modal-integration](spec:TASK:phase-4/git-panel-modal-integration)),
but the *placement* model is still Zed's: each panel is anchored to a
dock side, sized independently, and toggle-shown rather than tree-placed.

Two earlier attempts in the codon spec graph already touch this tension:

- [TASK:phase-3/agent-pane-conversion](spec:TASK:phase-3/agent-pane-conversion)
  — deferred because per-panel rewrites are too invasive when done
  one-off.
- [TASK:phase-4/git-status-pane](spec:TASK:phase-4/git-status-pane) —
  marked `wontdo` after a re-implementation of GitPanel as a pane
  duplicated ~85 % of the upstream view.

Phase 12 resolves both by going one level up: a single
`PanelItemAdapter` that hosts *any* `impl Panel` as a `workspace::Item`,
so the existing views keep their internals and gain a pane placement
for free. Dock placement survives as an opt-in *peek* — a transient,
auto-dismissing side rail for the cases where a sidebar genuinely
beats a pane (the agent thread mid-edit, the git commit editor while
staging hunks).

Refining requirements:

- [REQ:codon/panes-from-panels](spec:REQ:codon/panes-from-panels) —
  clauses `#c-adapter`, `#c-inventory`, `#c-peek-mode`,
  `#c-persistence`, `#c-keymap-surface`, `#c-dock-deprecation`,
  `#c-migration-prior-art`.
