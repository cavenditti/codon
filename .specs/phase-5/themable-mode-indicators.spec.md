---
id: TASK:phase-5/themable-mode-indicators
type: task
status: accepted
version: 0.0.1
summary: >
  Status-bar mode indicator becomes a prominent themed pill (bg + fg
  colors pulled from theme tokens) instead of inline text, so the
  active PaneMode is readable at a glance.
owners: [carlo]
progress: done
refines:
  - REQ:codon/modal-shell#c-mode-indicator-themable
---

# Visible, themable mode indicators

## What ships

The status-bar mode indicator becomes a prominent always-on pill:

- **Short labels**: `NOR` / `INS` / `CMD` instead of full words. Vim
  sub-modes (Visual / Replace / Operator-pending) still come through
  the indicator's `detail` channel and replace the short label —
  they're the more specific signal when active.
- **Per-mode saturated background**: prefers `vim_helix_normal_*` /
  `vim_insert_*` / `vim_replace_*` theme tokens; when a theme leaves
  those transparent (most non-Zed-shipped themes do) falls back to
  `theme.status()` accents — `info_background` / `success_background`
  / `warning_background` for Normal / Insert / Command respectively.
  These map to the conventional vim colour bar (blue / green /
  red-orange) and are guaranteed defined in every theme.
- **Bold foreground** from the matching `vim_*_foreground` or
  `status.info` / `success` / `warning`.
- **Command mode now actually triggers**: `CodonModeTracker` gains a
  `command_active: bool` flag, set by `CodonPalette::toggle` and
  cleared via `cx.on_release` on the modal entity. The indicator
  checks this flag *before* the vim-focused branch, so opening the
  palette flips to CMD regardless of which pane / vim mode was
  focused before.
- **Leftmost slot**: moved from mid-list to the very first left-item
  in the status bar.

## Files touched

- `crates/codon-mode/src/pane_mode.rs` — new `command_active` field.
- `crates/codon-mode/src/mode_indicator.rs` — short labels,
  status-token fallbacks, command-active branch.
- `crates/codon-command-palette/src/modal.rs` — flag set / release.
- `crates/codon-command-palette/Cargo.toml` — `codon-mode` dep.
- `apps/codon/src/zed.rs` — status-bar position reorder.
