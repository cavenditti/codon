---
id: TASK:phase-2/terminal-scrollback
type: task
status: accepted
version: 0.1.0
summary: >
  Persist last-N lines of terminal scrollback and respawn the shell on Enter.
owners: [carlo]
progress: deferred
refines:
  - REQ:codon/persistence#c-terminal-scrollback
---

# Terminal scrollback persistence (deferred)

Requires invasive changes in vendor/zed/crates/terminal_view (alacritty grid serialization + a dormant render mode). Working alternative: the user re-runs the last command. Tracked for a future phase.
