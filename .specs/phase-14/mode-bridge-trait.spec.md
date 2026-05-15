---
id: TASK:phase-14/mode-bridge-trait
type: task
status: draft
version: 0.0.1
summary: >
  Introduce a single `PaneModeBridge` trait in `codon-mode` and a
  central focus dispatcher; migrate every codon pane kind
  (terminal, file-manager, agent, jump, command-palette,
  cheatsheet) to update the mode tracker through that trait.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-mode-dispatch-hook
---

# Single hook for `CodonModeTracker` updates

## What changes

`CodonModeTracker` is updated from at least three different
patterns today:

- `crates/codon-command-palette/src/modal.rs:40-42` —
  `cx.update_global::<CodonModeTracker, _>(|t, _| t.set_command_active(true))`
  on open, mirrored on dismiss.
- `crates/file-manager/src/file_manager.rs` (focus subscriber path,
  ~line 4000 region) — sets the tracker's pane-mode on focus-in.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — does not touch
  the tracker at all (correct outcome but for the wrong reason —
  there is no convention saying it shouldn't).

The bug is the lack of a convention. This TASK introduces one.

## Approach

1. Add a `PaneModeBridge` trait to `crates/codon-mode/src/lib.rs`:

   ```rust
   pub trait PaneModeBridge {
       /// The pane-kind tag the tracker reports when this pane is focused.
       fn pane_mode(&self) -> PaneMode;
       /// Optional override: returns Some(true) to force command_active
       /// while this pane has focus. Default is None (no override).
       fn command_active_override(&self) -> Option<bool> { None }
   }
   ```

2. Add a central focus dispatcher in `codon-mode`:

   ```rust
   pub fn install_pane_mode_dispatcher(cx: &mut App) { ... }
   ```

   It subscribes to global focus changes; on each change, looks at the
   newly-focused entity, checks if it implements `PaneModeBridge`
   (via a registry keyed by `TypeId` populated at startup), and
   updates the tracker accordingly.

3. Each codon pane kind implements the trait and registers itself:
   - `Terminal` (codon side) — `PaneMode::Normal`.
   - `FileManager` — `PaneMode::Normal`.
   - `AgentPanel` — `PaneMode::Normal` (or its own variant if needed).
   - `JumpModal` — `PaneMode::Normal` with `command_active_override = Some(true)`.
   - `CommandPaletteModal` — `command_active_override = Some(true)`.
   - `CheatsheetModal` — `command_active_override = Some(false)`.

4. `apps/codon/src/zed.rs` calls `install_pane_mode_dispatcher(cx)`
   once at startup, after the workspace is built.

5. Per-crate manual `cx.update_global::<CodonModeTracker>` calls are
   removed. The dispatcher is the only writer.

## Coordination

This TASK depends on `modals-extract-scaffold` *only* for the
command-palette callsite — that line goes away in the scaffold
TASK and re-appears here as a `command_active_override`. If
landed before the scaffold TASK, the dispatcher takes over and the
scaffold TASK becomes a pure removal. Either order is fine.

## Non-goals

- Not changing the `PaneMode` enum's variants. The Normal / Insert /
  Command distinction stays as-is.
- Not changing the status-bar mode-indicator rendering. It reads the
  tracker; nothing about the read side moves.
- Not generalising to non-codon Zed panes (project_panel, etc.).
  Those are out of scope.

## Files touched

- `crates/codon-mode/src/lib.rs` — `PaneModeBridge` trait + dispatcher.
- `apps/codon/src/zed.rs` — single call to install the dispatcher.
- `crates/file-manager/src/file_manager.rs` — impl trait; remove the
  direct tracker update on focus.
- `crates/codon-command-palette/src/modal.rs` — impl trait; remove
  the inline `cx.update_global` at line 40.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — impl trait
  (`command_active_override = Some(false)`).
- `crates/codon-agent/` and `crates/codon-jump/` — impl trait.
- terminal pane wiring (where codon adapts Zed's terminal) — impl trait.

## Verification

- `cargo build -p codon` — clean.
- `cargo test -p codon-mode` — passes (add at least one test using a
  stub pane that constructs the dispatcher, swaps focus, and asserts
  the tracker transitions).
- `rg -n 'set_command_active|set_pane_mode' crates/` returns hits
  only inside `codon-mode`; no per-pane callsites.
- Manual smoke: focus cycles between terminal, file-manager, agent,
  command-palette, cheatsheet. The status-bar mode indicator
  reflects the focused pane every transition. Esc-dismissing the
  command palette restores the previous pane's mode within one
  frame.
