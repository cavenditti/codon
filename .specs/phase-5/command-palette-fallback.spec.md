---
id: TASK:phase-5/command-palette-fallback
type: task
status: accepted
version: 0.0.1
summary: >
  Commands without a registered Completer keep Zed's current
  behaviour: Enter dispatches the bare action immediately. The action
  is responsible for any follow-on picker it wants to open.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-palette#c-fallback
---

# Non-completer fallback

## What ships

Confirms that the palette doesn't *require* a completer to be useful:

- In Command mode, hitting `Enter` on a row dispatches the action
  exactly as Zed's `command_palette` does today
  (`CommandPaletteDelegate::confirm` model).
- If the action itself opens a picker (e.g. `outline::Toggle` opens
  the outline picker), that's fine and unchanged — codon's palette
  just hands control back to Zed.
- Typing `<space>` after a command that has *no* completer
  registered does not transition to Argument mode; the space goes
  into the query and the filter keeps narrowing the action list.
  (This makes the "is there a completer?" lookup the single decision
  point for the mode swap.)

This task exists mostly to lock in the behaviour as part of the
spec so reviewers can verify the fallback doesn't quietly regress
when later changes touch the argument flow.

## Reference points

- [`vendor/zed/crates/command_palette/src/command_palette.rs`](spec:src:vendor/zed/crates/command_palette/src/command_palette.rs)
  — `CommandPaletteDelegate::confirm`. The codon palette uses the
  same `dispatch_action` call for the no-completer path.

## Tests

- Manual: select a command with no completer (e.g.
  `editor::ToggleSoftWrap`), hit `Enter` — action runs and palette
  closes.
- Manual: type `toggle ` (trailing space) — the picker filters to
  toggles; nothing else happens.

Effort: low. ~30 LOC of decision logic + manual verification.
