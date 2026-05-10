---
id: REQ:codon/persistence
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Persistent session/layout state, on-quit flush, and unsaved-buffer
  recovery. Survives both clean quit and forced kill.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-2]
---

# Persistence

## Context

State that disappears at quit is hostile to a multiplexer workflow.
Codon persists sessions/windows/layouts to the KVP store and relies on
Zed's existing `SerializableItem` mechanism for editor/terminal
per-item state. Crash safety is best-effort: a 30-second heartbeat
plus an `on_app_quit` flush.

:::{requirement id="persistence" level="MUST"}
The system MUST provide:

- {#c-mutation-write} per-mutation persist (already done by action
  handlers via `persist_async`)
- {#c-heartbeat} a 30-second background task that re-persists the
  current registry
- {#c-shutdown-flush} an `on_app_quit` callback that writes one final
  snapshot
- {#c-rehydrate} `codon_session::init` loads the persisted registry
  from the KVP store at startup
- {#c-editor-restore} editors restore via Zed's existing
  `SerializableItem` impl, which already covers open files + cursor +
  scroll + dirty buffer contents (`EditorDb`)
- {#c-swap-files} unsaved-buffer recovery — covered by the same
  EditorDb mechanism (`ProjectSettings::session.restore_unsaved_buffers`,
  default true), no separate filesystem `.swp` sidecar added
- {#c-terminal-scrollback} terminal scrollback persistence and
  "Press Enter to respawn" — DEFERRED, requires invasive `terminal_view`
  + alacritty grid serialization changes
:::
