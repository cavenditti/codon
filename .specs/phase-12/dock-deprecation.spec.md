---
id: TASK:phase-12/dock-deprecation
type: task
status: accepted
version: 0.0.1
summary: >
  Stop registering Zed's three docks (left/right/bottom) as panel
  hosts in codon. Peek mode is the only dock surface that ships.
owners: [carlo]
progress: done
refines:
  - REQ:codon/panes-from-panels#c-dock-deprecation
---

# Dock deprecation in codon's Workspace init

## What changes

In `apps/codon/src/main.rs` (and any wiring under
[`vendor/zed/crates/workspace/src/codon_bridge.rs`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs)
or the codon-side workspace setup), the
`Workspace::add_panel::<AgentPanel>(…)` /
`add_panel::<ProjectPanel>` / `add_panel::<OutlinePanel>` /
`add_panel::<TerminalPanel>` / `add_panel::<GitPanel>` /
`add_panel::<DebugPanel>` / `add_panel::<CollabPanel>` calls are
removed (or never made). The `Workspace::left_dock` / `right_dock`
/ `bottom_dock` entities still exist on the `Workspace` struct
(removing them would touch hundreds of upstream callsites), but
they stay empty and unrendered in codon.

Side-effects:

- Cmd-J / Cmd-B / Cmd-? muscle-memory dock toggles inherited from
  Zed stop opening anything by default. The
  `panel-pane-keymap-surface` chords replace them.
- The Zed status bar's per-dock buttons (left / right / bottom dock
  toggles) disappear in codon. The codon status bar already owns
  this strip via the mode indicator + session indicator, so the
  loss is invisible.
- Persistence of dock state in the `workspace_*` SQLite tables
  becomes vestigial for codon — never read, never written, but
  the schema stays for upstream-diff cleanliness.

## Approach

1. Audit `apps/codon/src/main.rs` for all `add_panel::<…>` calls
   and delete them.
2. Audit any codon-side helpers (in `codon-session`, `codon-keymap`,
   etc.) that touch dock state — replace with no-ops or remove
   entirely.
3. Hide the three dock surfaces in the workspace render: codon
   should not display the dock chrome (resize handles, toggle
   buttons) when no panel is mounted. Investigate whether this
   needs a small render-time predicate in
   `vendor/zed/crates/workspace/src/workspace.rs` (e.g. only render
   a dock when `dock.panels().is_empty() == false`) or whether the
   existing render path already handles the empty case.
4. Regression check: launch codon, confirm no dock chrome is
   visible by default; invoke `codon_panes::PeekAgent`, confirm
   the peek surface from `peek-mode-transient-dock` appears
   instead of one of the legacy docks.

## Non-goals

- No removal of `Workspace::left_dock` / `right_dock` / `bottom_dock`
  fields. Too invasive against upstream; we keep them dormant.
- No new Zed setting for "hide docks". The behaviour is unconditional
  in codon's init.

## Files touched

- `apps/codon/src/main.rs` — drop `add_panel::<…>` calls and any
  related dock-config wiring.
- Possibly `vendor/zed/crates/workspace/src/workspace.rs` — guard
  the dock render path against empty docks (only if the current
  render path shows empty chrome).
- Test harness verification: existing `codon-session` snapshot
  tests pass unchanged (the dock fields were never serialized).
