---
id: TOPIC:topics/phase-11
type: topic
status: draft
version: 0.1.0
summary: >
  Window-nav tmux parity: last-window toggle, direct index goto,
  ergonomic 2-key motion, rename, safe close confirmation, and
  break-pane → new window.
owners: [carlo]
---

# Phase 11 — Window-nav tmux parity

The phase-2 / phase-5 work shipped the window data model, a fuzzy
switch picker, and a grid overview, but the day-to-day keystroke
surface still lags tmux. tmux users reach for four verbs that codon
doesn't yet bind ergonomically:

- `prefix l` — toggle between current and most-recently-active window.
- `prefix 0-9` — direct index goto. The action exists; just unbound.
- `prefix ,` — rename window. Action doesn't exist.
- `prefix !` — break the active pane into a new window of its own.

Plus two smaller gaps:

- `WindowNext` / `WindowPrev` are bound under `cmd-k shift-w …`,
  which is fine for discoverability but heavy for muscle memory.
- `WindowClose` silently kills dirty buffers because the workspace's
  save-prompt path never runs for the cascading close.

Refining requirements:

- [REQ:codon/windows](spec:REQ:codon/windows) — clauses `#c-last`,
  `#c-direct-index`, `#c-ergonomic-motion`, `#c-rename`,
  `#c-safe-close-confirm`, `#c-break-pane`.
