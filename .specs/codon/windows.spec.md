---
id: REQ:codon/windows
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Each session contains an ordered list of windows; one is visible at
  a time and the rest are reachable via keyboard / mouse.
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
  opens the same nested session→window tree as
  [REQ:codon/sessions#c-overview](spec:REQ:codon/sessions#c-overview),
  pre-positioned on the active window row. Each window row shows
  index, name, pane count, and a short layout shorthand
  (`|`/`-`/`≡`/etc. for the dominant split axis). The active
  window's layout is re-captured from the live workspace on open so
  pane count and shorthand reflect the current state, not the last
  switch-out snapshot. Mirrors tmux's `prefix w`.
:::

## Implementation

`WindowsStatusItem` reuses `ui::TabBar` with `Tab::end_slot(None)` for
the no-close-button look. Layout swap uses `workspace::codon_bridge`.
