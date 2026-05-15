---
id: TASK:phase-12/git-panel-migration
type: task
status: accepted
version: 0.0.1
summary: >
  Wire GitPanel through PanelItemAdapter — open as a pane by
  default, peek on the left side. Modal-integration patches stay.
owners: [carlo]
progress: done
refines:
  - REQ:codon/panes-from-panels#c-inventory
  - REQ:codon/panes-from-panels#c-keymap-surface
aspects: [inventory-verdict, open-and-peek-actions]
---

# GitPanel migration

## What ships

- `codon_panes::OpenGit` — construct (or reuse) the workspace's
  `GitPanel`, wrap it in `PanelItemAdapter`, insert it into the
  active pane.
- `codon_panes::PeekGit` — same panel, hosted by `peek_panel(…,
  PeekSide::Left, …)`.
- Default chord: `cmd-k g` (open) / `cmd-k shift-g` (peek). The
  legacy `cmd-k g s` rebinds to `OpenGit`.

## Preserves

The dispatch-context + mode-tracker patches from
[TASK:phase-4/git-panel-modal-integration](spec:TASK:phase-4/git-panel-modal-integration)
stay intact: the panel still publishes `pane_mode = "normal"` /
`"insert"` whether it's hosted by the adapter or by the peek
surface. The `[bindings.git_panel.normal]` and `.insert` blocks
remain valid for both placements.

## Approach

1. `GitPanel::load(workspace, cx)` (async constructor in
   [`vendor/zed/crates/git_ui/src/git_panel.rs`](spec:src:vendor/zed/crates/git_ui/src/git_panel.rs))
   is still the entry point. `OpenGit` awaits it and wraps in the
   adapter.
2. Commit-editor focus listener (the
   `cx.on_focus(&commit_editor_focus_handle, …)` from the
   modal-integration task) keeps working unchanged — it fires
   regardless of host.
3. Re-open behaviour: invoking `OpenGit` while the adapter tab is
   present focuses it; invoking while peeked closes the peek and
   re-mounts as a pane.

## Non-goals

- No change to GitPanel internals.
- No change to the `[bindings.git_panel.*]` keymap blocks; only the
  *entry* binding (`cmd-k g s`) shifts.

## Files touched

- `crates/codon-panes/src/git.rs` (new) — `OpenGit` / `PeekGit`.
- `crates/codon-panes/Cargo.toml` — add `git_ui` dep.
- `crates/codon-keymap/src/keymap.rs` — rebind `cmd-k g s` from
  `git_panel::ToggleFocus` to `codon_panes::OpenGit`; add the new
  `cmd-k g` / `cmd-k shift-g` chords.
