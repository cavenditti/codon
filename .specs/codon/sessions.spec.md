---
id: REQ:codon/sessions
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Codon provides tmux-style named sessions, each anchored to a cwd,
  with a layout snapshot per window and a fuzzy switch picker.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-2]
---

# Sessions

## Context

Codon is a single-OS-window multiplexer. The user creates named
sessions, each one a working context (cwd + layout + windows) that can
be switched between without losing state. One session is visible at a
time; switching swaps the workspace's center pane group in place.

:::{requirement id="sessions" level="MUST"}
The system MUST provide named sessions with:

- {#c-data-model} a Session struct holding `id`, `name`, `cwd`,
  `windows: Vec<Window>`, `active_window: usize`, `last_attached_ms`
- {#c-create} an action `SessionNew` that creates a session named after
  the current project's primary cwd (with a numeric suffix on collision)
- {#c-switch} an action `SessionSwitch` that opens a fuzzy picker over
  existing sessions and swaps in the chosen one
- {#c-close} an action `SessionClose` that removes the active session
  (refuses to remove the last one)
- {#c-status-bar} a status-bar indicator showing the active session
  name on the left
- {#c-persistence} JSON-serialized persistence in the global KVP store
  under key `codon_sessions_v1`
- {#c-overview} a tmux-style overview action (`SessionOverview`) that
  shows every session and its windows as a single nested tree — one
  line per row, session rows show name/cwd/window-count/last-attached,
  window rows show index/name/pane-count/layout shorthand. `j`/`k`
  move between visible rows, `h`/`l` collapse/expand a session, Enter
  attaches the selected session (or session+window if a window row is
  highlighted). Mirrors tmux's `prefix s` tree view. Shares the
  underlying modal with [REQ:codon/windows#c-overview](spec:REQ:codon/windows#c-overview)
  — the two actions differ only in initial selection.
:::

## Implementation

The codon-session crate at `crates/codon-session/` houses all of this.
The KVP write happens after every mutation via a background spawn, plus
a 30-second heartbeat (see [REQ:codon/persistence](spec:REQ:codon/persistence)).
