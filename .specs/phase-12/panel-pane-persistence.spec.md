---
id: TASK:phase-12/panel-pane-persistence
type: task
status: accepted
version: 0.0.1
summary: >
  Adapter-hosted panels round-trip through LayoutSnapshot so they
  restore alongside terminals, editors, and the file manager.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-persistence
---

# Panel-as-pane persistence

## What ships

Every adapter-hosted panel survives:

- Window-switching via the codon-session in-memory pane stash
  (`WindowRuntimeCache` in
  [`crates/codon-session/src/runtime.rs`](spec:src:crates/codon-session/src/runtime.rs))
  — same path terminals already use.
- Cross-restart restoration via `LayoutSnapshot` JSON in the global
  KVP under `codon_sessions_v1`.

## Approach

1. The adapter's `Item::serialized_item_kind` returns the panel's
   `persistent_name()` — that string is what `LayoutSnapshot` keys
   off when re-instantiating panes during `apply_layout`.
2. `codon_bridge::capture_layout` already encodes any
   `Box<dyn ItemHandle>` whose `serialized_item_kind` is non-`None`,
   so the capture side needs no new code.
3. `apply_layout` needs a small dispatch table: given a kind string
   plus serialized state, return a `Box<dyn ItemHandle>` for the
   matching panel. This is the only new vendored-Zed surface — a
   `codon_bridge::register_panel_restorer(kind, factory_fn)`
   registry, populated by codon-panes during `init` for each
   converted panel.
4. Panels that already implement `SerializableItem` (TerminalPanel
   would, but it's dropped; GitPanel doesn't appear to today;
   AgentPanel has its own thread persistence outside the Item
   serializer) keep working: per-panel state is restored by the
   panel's own constructor reading the same backing store it
   already uses. The Item-level state we round-trip is just *which
   panel*, not the panel's contents.
5. Test: capture a layout containing an `AgentPanel` adapter and a
   terminal; round-trip through serde JSON; `apply_layout` and
   assert both panes restore.

## Non-goals

- No new per-panel serialization. Each panel keeps whatever
  persistence it has upstream; we only add the indirection that
  lets `LayoutSnapshot` find the right factory by kind.
- No peek restoration. `LayoutSnapshot` deliberately ignores peeks
  (see `peek-mode-transient-dock`).

## Files touched

- `vendor/zed/crates/workspace/src/codon_bridge.rs` — add the
  `register_panel_restorer` registry + lookup in `apply_layout`.
- `crates/codon-panes/src/lib.rs` — `init(cx)` calls
  `register_panel_restorer(P::persistent_name(), …)` for each
  converted panel.
- `crates/codon-session/src/runtime.rs` — verify the in-memory
  pane stash treats adapter items the same as any other Item (no
  code change expected; this is a verification step).
