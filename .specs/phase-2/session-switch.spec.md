---
id: TASK:phase-2/session-switch
type: task
status: accepted
version: 0.1.0
summary: >
  Fuzzy session picker driven by SessionPickerDelegate.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-switch
---

# Session switch picker

Implemented in `crates/codon-session/src/picker.rs` (SessionPickerDelegate + SessionSwitchModal). On confirm, marks the chosen session active and updates the workspace's session_id; layout swap happens via window switching.
