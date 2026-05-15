---
id: TASK:phase-12/agent-panel-migration
type: task
status: accepted
version: 0.0.1
summary: >
  Wire AgentPanel through PanelItemAdapter — open as a pane by
  default, peek on the right side.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-inventory
  - REQ:codon/panes-from-panels#c-keymap-surface
aspects: [inventory-verdict, open-and-peek-actions]
---

# AgentPanel migration

## What ships

- `codon_panes::OpenAgent` — constructs (or reuses) the workspace's
  `AgentPanel`, wraps it in `PanelItemAdapter`, inserts it into the
  active pane (or focuses the existing adapter tab if already
  open).
- `codon_panes::PeekAgent` — same panel entity, but mounted via
  `peek_panel(…, PeekSide::Right, …)`.
- Default cmd-k chord: `cmd-k a` (open) / `cmd-k shift-a` (peek).

## Approach

1. AgentPanel construction stays where it is today
   ([`vendor/zed/crates/agent_ui/src/agent_panel.rs`](spec:src:vendor/zed/crates/agent_ui/src/agent_panel.rs))
   — its `load(workspace, cx)` async constructor is the entry
   point. The codon dispatch closure for `OpenAgent` awaits the
   load and then hands the resulting `Entity<AgentPanel>` to the
   adapter.
2. The cross-pane verbs from
   [REQ:codon/agent-pane](spec:REQ:codon/agent-pane) keep working
   unchanged: `seed_explain_with_selection` is called on the panel
   entity, not on its host. After seeding, the dispatch focuses
   *the adapter pane* (or opens it if not present), not the dock.
3. Singleton guarantee: codon-panes maintains a workspace-scoped
   `Option<Entity<AgentPanel>>` keyed by workspace id so opening
   the agent twice doesn't construct two panels.

## Non-goals

- No change to AgentPanel internals.
- No change to the seed-helper surface — the existing
  `seed_explain_with_selection` is sufficient.

## Files touched

- `crates/codon-panes/src/agent.rs` (new) — `OpenAgent` /
  `PeekAgent` action dispatch.
- `crates/codon-panes/Cargo.toml` — add `agent_ui` dep.
- Embedded TOML in `crates/codon-keymap/src/keymap.rs` — the two
  cmd-k chords for agent.
