---
id: TASK:phase-12/debug-panel-migration
type: task
status: accepted
version: 0.0.1
summary: >
  Wire DebugPanel through PanelItemAdapter — open as a pane by
  default, peek on the bottom.
owners: [carlo]
progress: done
refines:
  - REQ:codon/panes-from-panels#c-inventory
  - REQ:codon/panes-from-panels#c-keymap-surface
aspects: [inventory-verdict, open-and-peek-actions]
---

# DebugPanel migration

## What ships

- `codon_panes::OpenDebug` — construct (or reuse) the workspace's
  `DebugPanel`, wrap it in `PanelItemAdapter`, insert into the
  active pane.
- `codon_panes::PeekDebug` — `peek_panel(…, PeekSide::Bottom, …)`.
- Default chord: `cmd-k d` (open) / `cmd-k shift-d` (peek).

## Note

Debugging isn't an active codon workflow today — this task exists
to ensure the inventory in
`REQ:codon/panes-from-panels#c-inventory` is fully covered. Cost
is the same as the other migrations: one action pair, one keymap
entry pair.

## Approach

1. `DebugPanel::load(workspace, cx)` is the construction entry.
2. Verify `debugger_ui::init(cx)` is called in
   `apps/codon/src/main.rs`. If not, add it.
3. Singleton mirrors the other migrations.

## Known clash

`cmd-k d g` is bound to `diagnostics::Deploy` per
[TASK:phase-5/diagnostics-pane](spec:TASK:phase-5/diagnostics-pane).
The new `cmd-k d` opens the debug *pane*; the existing
`cmd-k d g` (diagnostics) still works because cmd-k is a chord
root and `d` alone vs `d g` are disambiguated by the chord timeout
([`gpui::set_keystroke_chord_timeout`](spec:src:vendor/zed/crates/gpui/src/keymap.rs)
is set to 5 s in codon). If this proves too slow during prototyping,
the debug chord moves to `cmd-k shift-d` only (peek-only — drop
the pane bindings) and `cmd-k d` reverts to the diagnostics chord
root. Note this in `panel-inventory-decision` if the verdict shifts.

## Non-goals

- No new debugger features. Exposing the upstream surface only.

## Files touched

- `crates/codon-panes/src/debug.rs` (new) — `OpenDebug` /
  `PeekDebug`.
- `crates/codon-panes/Cargo.toml` — add `debugger_ui` dep.
- `crates/codon-keymap/src/keymap.rs` — chord bindings.
- `apps/codon/src/main.rs` — `debugger_ui::init(cx)` if missing.
