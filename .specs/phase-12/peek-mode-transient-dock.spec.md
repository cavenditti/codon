---
id: TASK:phase-12/peek-mode-transient-dock
type: task
status: accepted
version: 0.0.1
summary: >
  Single transient "peek" dock surface for on-demand sidebar viewing
  of converted panels. Off by default; auto-dismisses on focus-loss
  or esc; never persists.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-peek-mode
---

# Peek mode — transient dock surface

## What ships

A `PeekDock` widget owned by `codon-panes` that hosts a single panel
view at a time on one of three sides (left / right / bottom). At
most one peek is visible per workspace; opening a second peek
replaces the first.

```rust
pub enum PeekSide { Left, Right, Bottom }

pub fn peek_panel<P: Panel>(
    workspace: &mut Workspace,
    panel: Entity<P>,
    side: PeekSide,
    window: &mut Window,
    cx: &mut Context<Workspace>,
);
```

## Behaviour

- A peek mounts the panel into a floating-style dock anchored to the
  requested side, sized at the panel's `default_size`.
- The peek auto-dismisses on:
  - `esc` while the peek surface has focus.
  - Focus leaving the peek (any pane in the workspace tree gets
    focus): the peek closes after a one-frame debounce so click /
    keystroke through the peek into a pane doesn't re-fire the
    dismiss.
  - An explicit `codon_panes::PeekDismiss` action.
- Re-invoking the same `Peek<Name>` action while that panel is
  already peeked toggles the peek closed (mirroring tmux popup
  behaviour).
- Peeks do **not** participate in `LayoutSnapshot` —
  `capture_layout` ignores them, `apply_layout` never restores one.
- Peek mode is *off by default*: every panel that supports peek
  declares so explicitly via its `Peek<Name>` registration; calling
  the action is the only way a peek opens.

## Approach

1. New module `crates/codon-panes/src/peek.rs` owning the `PeekDock`
   entity and the side enum. Reuse Zed's `Dock` *visual* primitives
   if convenient (resize handle, shadow), but the peek widget is
   codon-owned — not registered as one of `Workspace::left_dock` /
   `right_dock` / `bottom_dock` (those are deprecated in
   `dock-deprecation`).
2. Focus tracking: subscribe to workspace focus changes
   (`Workspace::on_focus`) and close the peek when focus moves
   outside of it. Use a one-frame `cx.defer` for the dismiss so
   that mouse-down → focus-loss → click-target-receives-input
   sequences don't fight the auto-close.
3. `codon_panes::PeekDismiss` action, bound to `esc` while the peek
   has focus (registered through `[bindings.peek_dock.normal]` in
   codon-keymap — naming consistent with `[bindings.git_panel.*]`).
4. Unit / integration test: open a peek over the test panel, focus
   a pane, assert the peek closed; re-open, press `esc`, assert
   closed.

## Non-goals

- No restoration. Peeks are ephemeral by contract.
- No per-side independent peeks. One peek surface at a time —
  invoking a second `Peek<Name>` action replaces the first
  regardless of side.
- No peek into a peek (panels-inside-peeks). The peek only hosts
  one `Box<dyn PanelHandle>` at a time.

## Files touched

- `crates/codon-panes/src/peek.rs` (new).
- `crates/codon-panes/src/lib.rs` — export `peek_panel`,
  `PeekDismiss`.
- `crates/codon-keymap/src/keymap.rs` — `[bindings.peek_dock.normal]`
  with the `esc` → `PeekDismiss` arm, plus resolver entry.
