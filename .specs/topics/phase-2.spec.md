---
id: TOPIC:topics/phase-2
type: topic
status: accepted
version: 0.1.0
summary: >
  Tmux-style sessions, windows-within-session, layout snapshots,
  keyboard pane navigation/resize, and persistence hardening.
owners: [carlo]
---

# Phase 2 — Sessions, layout, persistence

Codon needs to feel like a multiplexer: the user creates named sessions
(each anchored to a cwd), groups panes into windows within a session,
and switches between them with a keystroke. Crashes and quits must not
lose layout state or unsaved buffer contents.

Refining requirements:

- [REQ:codon/sessions](spec:REQ:codon/sessions) — session registry +
  switch + status bar.
- [REQ:codon/windows](spec:REQ:codon/windows) — windows-in-session +
  status bar tab strip.
- [REQ:codon/layout](spec:REQ:codon/layout) — layout snapshots, stack
  variant, keyboard resize.
- [REQ:codon/pane-ux](spec:REQ:codon/pane-ux) — pane focus / move,
  default split kind, native dialog audit.
- [REQ:codon/persistence](spec:REQ:codon/persistence) — heartbeat,
  shutdown flush, terminal scrollback, swap files.
