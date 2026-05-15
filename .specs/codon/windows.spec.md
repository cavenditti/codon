---
id: REQ:codon/windows
type: requirement
status: accepted
version: 0.2.0
level: MUST
summary: >
  Each session contains an ordered list of windows; one is visible at
  a time and the rest are reachable via keyboard / mouse, with
  tmux-parity verbs for direct, last, rename, and break-pane motion.
owners: [carlo]
refines: [REQ:codon/sessions#c-data-model]
categorized_under: [TOPIC:topics/phase-2]
---

# Windows within a session

## Context

Sessions group windows the way tmux does: each window holds its own
center pane group. Switching windows is layout swap with no cwd change.

:::{requirement id="windows" level="MUST"}
The system MUST manage windows-within-session:

- {#c-data-model} `Window { id, name, layout: Option<LayoutSnapshot> }`
  stored in `Session.windows`
- {#c-actions} actions `WindowNew`, `WindowNext`, `WindowPrev`,
  `WindowClose`, and parameterized `WindowGoto(usize)`
- {#c-status-bar} a tab-bar-shaped status bar indicator with one tab
  per window, no close-X, with click-to-switch
- {#c-swap-on-switch} switching captures the outgoing window's layout
  snapshot before applying the incoming window's snapshot
- {#c-switch-picker} a fuzzy picker action (`WindowSwitch`) for
  jumping by name across the active session's windows — same picker
  shape as `SessionSwitch` but scoped to the current session
- {#c-overview} a tmux-style overview action (`WindowOverview`) that
  renders every window in the active session as a thumbnail tile —
  name, dominant pane kind, layout preview — with arrow / hjkl
  navigation and Enter to switch. Mirrors tmux's `prefix w`.
- {#c-last} a `WindowLast` action that toggles between the active
  window and the most-recently-active window in the same session.
  Mirrors tmux's `prefix l`. The session tracks the previous window
  index alongside the current one; any switch — picker, overview,
  cycle, goto, or status-bar click — updates it.
- {#c-direct-index} `WindowGoto(usize)` MUST be reachable from the
  default keymap at indices 1–9. Indices are 1-based to match tmux;
  the action stores zero-based internally. Out-of-range indices are
  no-ops with a debug log, never a panic.
- {#c-ergonomic-motion} default keymap MUST expose 2-key motion for
  `WindowNext` / `WindowPrev` / `WindowLast`. The existing 3-key
  `cmd-k shift-w …` chords stay as the discoverable "windows menu",
  but the common verbs also work with a shorter chord.
- {#c-rename} a `WindowRename` action that opens a single-line text
  prompt seeded with the current window's name. Empty input cancels.
  Duplicate names within the same session are allowed (windows are
  identified by id, not name); the indicator appends a short `#id`
  suffix when names collide.
- {#c-safe-close-confirm} `WindowClose` MUST prompt before closing a
  window that contains panes with dirty (unsaved) items. The prompt
  reuses workspace's existing save-prompt machinery rather than a
  bespoke dialog. Clean windows close without confirmation.
- {#c-break-pane} a `BreakPaneToWindow` action that promotes the
  active pane in the current window into a new window of its own.
  Implemented at the `LayoutSnapshot` level: split the current
  layout into (remaining, broken) trees, write the remaining tree
  back to the current window, create a new window whose layout is
  the broken-out pane, switch to it. The broken pane's items
  re-hydrate via `SerializableItemRegistry`, so editor buffers and
  terminal connections survive intact. Mirrors tmux's `prefix !`.
:::

## Implementation

`WindowsStatusItem` reuses `ui::TabBar` with `Tab::end_slot(None)` for
the no-close-button look. Layout swap uses `workspace::codon_bridge`.
`previous_window` lives on `Session` (not in `WindowRuntimeCache`) so
it persists across restarts.
