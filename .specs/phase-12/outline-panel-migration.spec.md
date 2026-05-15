---
id: TASK:phase-12/outline-panel-migration
type: task
status: accepted
version: 0.0.1
summary: >
  Wire OutlinePanel through PanelItemAdapter — open as a pane by
  default, peek on the left side.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-inventory
  - REQ:codon/panes-from-panels#c-keymap-surface
aspects: [inventory-verdict, open-and-peek-actions]
---

# OutlinePanel migration

## What ships

- `codon_panes::OpenOutline` — construct (or reuse) the workspace's
  `OutlinePanel`, wrap it in `PanelItemAdapter`, insert into the
  active pane.
- `codon_panes::PeekOutline` — `peek_panel(…, PeekSide::Left, …)`.
- Default chord: `cmd-k o` (open) / `cmd-k shift-o` (peek).

## Why this matters

`OutlinePanel`
([`vendor/zed/crates/outline_panel/src/outline_panel.rs`](spec:src:vendor/zed/crates/outline_panel/src/outline_panel.rs))
gives a symbol outline + file tree view today entirely unused by
codon. The cost to expose it is the same as any other panel under
this Phase: one action pair plus two keymap entries.

## Approach

1. `OutlinePanel::load(workspace, cx)` is the construction entry.
2. Confirm `outline_panel::init(cx)` is called in
   `apps/codon/src/main.rs`. If not, add it during this task.
3. Singleton guarantee mirrors `agent-panel-migration`: one
   `Entity<OutlinePanel>` per workspace.
4. j/k navigation: confirm the existing key dispatch context
   ("OutlinePanel") works under codon's keymap. If a
   `[bindings.outline_panel.normal]` block is needed to wire
   `j` / `k` / `enter`, add it during this task — same pattern as
   `[bindings.git_panel.normal]`.

## Non-goals

- No new outline features. We are exposing the upstream surface,
  not extending it.

## Files touched

- `crates/codon-panes/src/outline.rs` (new) — `OpenOutline` /
  `PeekOutline`.
- `crates/codon-panes/Cargo.toml` — add `outline_panel` dep.
- `crates/codon-keymap/src/keymap.rs` — chord bindings; possibly
  a `[bindings.outline_panel.normal]` block.
- `apps/codon/src/main.rs` — `outline_panel::init(cx)` if missing.
