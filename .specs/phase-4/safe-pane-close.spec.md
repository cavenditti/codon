---
id: TASK:phase-4/safe-pane-close
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the default cmd-w action with a codon-owned SafeCloseActiveItem
  that closes the active item, falls back to closing the pane / codon
  window, and ends with replace_center_with_empty_pane — never the OS
  window.
owners: [carlo]
progress: done
refines:
  - REQ:codon/pane-ux#c-safe-close
---

# Safe `cmd-w` close

## Why

Zed's default `cmd-w` → `pane::CloseActiveItem`. When the active pane
becomes empty, `pane::close_active_item` (line ~1599 of
[vendor/zed/crates/workspace/src/pane.rs](spec:src:vendor/zed/crates/workspace/src/pane.rs))
dispatches `workspace::CloseWindow` if
`WorkspaceSettings::when_closing_with_no_tabs.should_close()` —
collapsing the whole OS window. Plus default macOS keymap has
`cmd-w` → `workspace::CloseWindow` in non-Editor contexts
(`vendor/zed/assets/keymaps/default-macos.json` lines 443, 1495). In
codon's single-OS-window multiplexer that's a destructive surprise.

## What ships

A new action `codon_session::SafeCloseActiveItem` whose handler does:

1. Active pane has more than one item → close just the active item
   (delegate to `pane::CloseActiveItem`).
2. Workspace has more than one pane → close the pane via
   `Workspace::remove_pane`.
3. Active session has more than one window → close the active window
   (delegate to existing `codon_session::WindowClose`).
4. None of the above → call
   `Workspace::replace_center_with_empty_pane` (helper added during
   the session-runtime work — see
   [TASK:phase-2/session-actions](spec:TASK:phase-2/session-actions)).
   The OS window stays open.

`cmd-w` and `cmd-k w` in codon's keymap point at the new action.
`cmd-shift-w` and `cmd-q` stay as-is for explicit window / app close.

## Defense in depth

In `vendor/zed/crates/workspace/src/pane.rs::close_active_item`, gate
the `dispatch_action(CloseWindow)` branch behind a process-wide flag
(set false in codon at startup, similar pattern to
`gpui::set_keystroke_chord_timeout`). Catches any reachable upstream
code path that might still call `pane::CloseActiveItem` directly.

## Files

- `crates/codon-session/src/actions.rs` — add the action + handler,
  reusing existing helpers.
- `crates/codon-keymap/src/keymap.rs` — bind `cmd-w` and `cmd-k w` to
  the new action; add a `bind!` arm for it.
- `assets/config/keymap.example.toml` — mirror the binding.
- `vendor/zed/crates/workspace/src/pane.rs` + a small static flag
  module (in workspace or gpui) for the defense-in-depth flag.

Single commit, single PR per repo.
