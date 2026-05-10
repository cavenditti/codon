---
id: TASK:phase-2/windows-status-indicator
type: task
status: accepted
version: 0.1.0
summary: >
  Tab-bar-shaped status item with one tab per window, no close-X, click-to-switch.
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-status-bar
---

# Windows status bar indicator

Implemented in `crates/codon-session/src/window_indicator.rs`. Reuses `ui::TabBar` and `ui::Tab` with `end_slot(None)` to omit the close button.
