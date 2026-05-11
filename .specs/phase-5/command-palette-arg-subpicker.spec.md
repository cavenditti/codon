---
id: TASK:phase-5/command-palette-arg-subpicker
type: task
status: accepted
version: 0.0.1
summary: >
  After space is typed past a completer-registered command, the
  palette transitions into argument mode — the query feeds the
  Completer, Enter builds and dispatches the action, Esc returns to
  command mode.
owners: [carlo]
progress: done
refines:
  - REQ:codon/command-palette#c-arg-subpicker
---

# Argument sub-picker mode

## What ships

State machine in the codon command palette modal:

```text
Command  ── space typed after a completer-registered match ──▶  Argument
                                                ◀─── Esc ────
```

Concrete behaviour:

- The palette tracks `mode: Mode` where
  `Mode = Command | Argument { action_name, completer, partial: String }`.
- In `Command` mode the picker is the existing
  `CommandPaletteDelegate`-like list of all actions.
- Transitioning to `Argument`: when the query parses as
  `<command_name> <rest>` and a completer is registered for
  `command_name`, the palette swaps the delegate for a
  `ArgumentPickerDelegate` whose `query` is `rest` and whose
  `update_matches` calls `completer.complete(query, cx)`.
- `Enter` in `Argument` mode calls
  `completer.build_action(selected_value)` and dispatches it via the
  workspace's focused dispatch chain (mirroring how
  `CommandPaletteDelegate::confirm` dispatches today).
- `Esc` in `Argument` mode returns to `Command` mode with the
  command name still in the query (no information loss).
- The description pane (see
  `command-palette-description-pane`) keeps showing the parent
  command's doc + placeholder string from the completer.

## Reference points

- [`vendor/zed/crates/command_palette/src/command_palette.rs`](spec:src:vendor/zed/crates/command_palette/src/command_palette.rs)
  — `CommandPaletteDelegate::confirm` is the model for dispatch.
- [`vendor/zed/crates/picker/src/picker.rs`](spec:src:vendor/zed/crates/picker/src/picker.rs)
  — swapping delegates inside one modal: prefer one outer modal
  that holds *either* picker rather than nesting Pickers.

## Tests

- Manual: open palette, type `open ` (with the trailing space), see
  the picker swap to a file list. Type to filter, `Enter` opens.
- Manual: `Esc` from argument mode keeps you in the palette on the
  parent command row.
- Manual: typing a space after a command with *no* registered
  completer keeps the existing Command-mode picker behaviour.

Effort: medium-large. ~250 LOC for the state machine + the two
delegates wired together.
