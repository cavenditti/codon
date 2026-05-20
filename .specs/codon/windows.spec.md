---
id: REQ:codon/windows
type: requirement
status: accepted
version: 0.3.0
level: MUST
summary: >
  Each session contains a fixed set of window slots — all conceptually
  present, only visible when populated — reachable via keyboard / mouse,
  with tmux-parity verbs for direct, last, rename, and break-pane motion.
owners: [carlo]
refines: [REQ:codon/sessions#c-data-model]
categorized_under: [TOPIC:topics/phase-2]
---

# Windows within a session

## Context

Sessions group windows the way tmux does: each window holds its own
center pane group. Switching windows is layout swap with no cwd change.
Codon's mental model is that **windows always exist** — every session
carries a fixed pool of slots, one per digit-keyed binding (`prefix 1`
… `prefix 9`). A slot is materialised the first time the user puts
something into it and stays visible only while it is in use; the rest
remain invisible scaffolding until claimed.

:::{requirement id="windows" level="MUST"}
The system MUST manage windows-within-session:

- {#c-data-model} `Window { id, name, layout: Option<LayoutSnapshot> }`
  stored in `Session.windows`
- {#c-fixed-slots} every session MUST carry exactly `WINDOW_SLOTS`
  windows (currently 9, matching the digit-keyed bindings 1-9) from
  the moment `Session::new` runs. Persisted sessions saved under
  earlier models MUST be padded up to this invariant on load. Slots
  are addressed by 0-based index; ids stay stable so cache lookups,
  `previous_window`, and the runtime cache key remain valid across
  pad-on-load and clear operations.
- {#c-emptiness-rule} a window slot is "in use" iff its persisted
  layout contains at least one item OR its name has been changed
  from the default-for-index (`(idx+1).to_string()`). Every other
  slot is "empty" and excluded from the indicator, picker, overview,
  and cycle navigation. The active slot is *always* shown even when
  empty so the user can tell where they are.
- {#c-actions} actions `WindowNew`, `WindowNext`, `WindowPrev`,
  `WindowClose`, and parameterized `WindowGoto(usize)`. Under the
  fixed-slots invariant the verbs MUST behave as follows:
  - `WindowNew` hops to the lowest-indexed empty slot. If every
    slot is in use, it surfaces a toast and stays put.
  - `WindowNext` / `WindowPrev` cycle through the **displayed**
    slots only (so empty scaffolding is invisible to cycling).
  - `WindowGoto(N)` always switches — empty targets materialise as
    a welcome pane on arrival.
  - `WindowClose` clears the active slot (drops its layout, resets
    its name to the default, evicts the runtime cache entry) and
    lands on the most-recently-active populated slot, or slot 0 if
    none. The slot itself never disappears.
- {#c-status-bar} a tab-bar-shaped status bar indicator that shows
  every populated slot (plus the active slot when empty), no close-X,
  with click-to-switch. Empty slots stay hidden until claimed.
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
- {#c-last} a `WindowLast` action that toggles between the active
  window and the most-recently-active window in the same session.
  Mirrors tmux's `prefix l`. The session tracks the previous window
  index alongside the current one; any switch — picker, overview,
  cycle, goto, or status-bar click — updates it.
- {#c-direct-index} `WindowGoto(usize)` MUST be reachable from the
  default keymap at indices 1–9. Indices are 1-based to match tmux;
  the action stores zero-based internally. Indices in `0..WINDOW_SLOTS`
  always resolve to a real slot (empty slots are materialised on
  arrival); higher indices are no-ops with a debug log, never a panic.
- {#c-ergonomic-motion} default keymap MUST expose 2-key motion for
  `WindowNext` / `WindowPrev` / `WindowLast`. The existing 3-key
  `cmd-k shift-w …` chords stay as the discoverable "windows menu",
  but the common verbs also work with a shorter chord.
- {#c-rename} a `WindowRename` action that opens a single-line text
  prompt seeded with the current window's name. Empty input cancels.
  Duplicate names within the same session are allowed (windows are
  identified by id, not name); the indicator appends a short `#id`
  suffix when names collide.
- {#c-safe-close-confirm} `WindowClose` MUST prompt before clearing
  a slot that contains panes with dirty (unsaved) items. The prompt
  reuses workspace's existing save-prompt machinery rather than a
  bespoke dialog. Clean slots clear without confirmation; already-
  empty slots are an explicit no-op rather than a confusing
  pseudo-action.
- {#c-break-pane} a `BreakPaneToWindow` action that promotes the
  active pane in the current window into the next empty slot.
  Implemented at the `LayoutSnapshot` level: split the current
  layout into (remaining, broken) trees, write the remaining tree
  back to the current window, plant the broken pane into the
  lowest-indexed empty slot, switch to it. The broken pane's items
  re-hydrate via `SerializableItemRegistry`, so editor buffers and
  terminal connections survive intact. If every slot is in use, the
  action surfaces a toast and stays put. Mirrors tmux's `prefix !`.
:::

## Implementation

`WindowsStatusItem` reuses `ui::TabBar` with `Tab::end_slot(None)` for
the no-close-button look. Layout swap uses `workspace::codon_bridge`.
`previous_window` lives on `Session` (not in `WindowRuntimeCache`) so
it persists across restarts.
