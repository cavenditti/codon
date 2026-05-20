---
id: TASK:phase-20/window-indicator-kind-glyph
type: task
status: accepted
version: 0.1.0
summary: >
  Window indicator tabs always lead with the tmux jump number and
  carry a kind glyph: a colored dot + short fallback label derived
  from the window's active pane when the user has not renamed it.
owners:
  - codon-core
progress: done
refines:
  - REQ:codon/windows#c-status-bar
assignee:
eta:
blocked_by: []
---

# Window indicator kind glyph

## Plan

Today [`WindowsStatusItem::render`](spec:src:crates/codon-session/src/window_indicator.rs:30-72)
shows each window as a `Tab` whose only child is `Label::new(win.name)`.
The auto-name applied at window creation is the numeric `WindowId`
(see [`Session::add_window`](spec:src:crates/codon-session/src/session.rs:135-140)),
which (a) silently diverges from the index-based `WindowGoto`
binding once any window is removed, and (b) carries no signal about
the window's purpose.

Re-shape the tab content as:

1. **Lead numeral.** The first character is `idx + 1`, matching the
   `prefix N` chord wired in
   [`keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs:304-314).
   Always rendered — even when the user has supplied a custom name —
   so the jump key stays visible.
2. **Tail label.** If `win.name` differs from both the index-derived
   auto-name (`format!("{}", idx+1)`) and the id-derived auto-name
   (`format!("{}", win.id.0)`), show `win.name` verbatim. Otherwise
   derive a short kind label from the window's active pane:
   - `Terminal` → `"term"`
   - `FileManager` → `"FM"`
   - `GitPanel` → `"git"`
   - `AgentPanel` → `"agent"`
   - `Outline Panel` → `"outline"`
   - `DebugPanel` → `"debug"`
   - `Editor` (or any other kind) → `"edit"`
   - unknown / empty pane → no tail (just the numeral)
3. **Kind glyph.** A subtle `Indicator::dot()` in the tab's
   `start_slot`, colored by the same kind classification:
   - terminal → `Color::Success`
   - fm → `Color::Info`
   - git → `Color::Warning`
   - agent → `Color::Accent`
   - outline → `Color::Muted`
   - debug → `Color::Error`
   - editor / fallback → `Color::Default`

For the workspace-active window read the kind from
`workspace.active_pane().read(cx).active_item()` via the same
`downcast` ladder used by
[`pane_context_label::caption_for`](spec:src:crates/codon-session/src/pane_context_label.rs:56-104).
For the other windows traverse `win.layout` (LayoutSnapshot) and
extract the `ItemSnapshot.kind` of the first pane carrying
`active: true` — see
[`LayoutSnapshot`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs:28-104).
Process-name resolution for terminal panes is deferred — the kind
label alone is enough to satisfy the "what's running" UX ask.

## Acceptance

- Every tab in the windows status item renders `idx+1` at the start
  even when the user has named the window.
- Renaming a window to anything other than `"{idx+1}"` /
  `"{id.0}"` causes that name to replace the kind label tail.
- Opening a terminal in a window with no user-supplied name renders
  `"N term"` with a green dot; switching that pane to a file
  manager re-renders to `"N FM"` with a blue dot.
- Inactive windows still surface the correct kind label/dot from
  their persisted `LayoutSnapshot` (verified by switching away and
  observing the tab unchanged).
- `cargo test -p codon-session` stays green; new unit coverage
  exercises the kind-label classifier against synthetic snapshots
  and the auto-name detector against both numeric forms.
