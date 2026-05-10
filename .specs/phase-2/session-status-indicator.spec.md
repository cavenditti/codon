---
id: TASK:phase-2/session-status-indicator
type: task
status: accepted
version: 0.1.0
summary: >
  SessionStatusItem renders the active session's name on the left of the status bar.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-status-bar
---

# Session status bar indicator

Lives at `crates/codon-session/src/status_item.rs`. Mounted in `apps/codon/src/zed.rs` during the workspace observer.
