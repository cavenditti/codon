---
id: TASK:phase-24/fm-operation-undo
type: task
status: accepted
version: 0.1.0
summary: >
  Record reversible FM operations and expose safe undo from completion
  notifications and task history.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-stateful-ux#c-operation-undo
blocked_by: []
---

# Undoable file operations

Extend task history with an optional inverse plan for rename, move,
trash, and paste. Completion notifications expose Undo while the plan
is valid; task history provides the same action.

## Acceptance

- Successful single and bulk operations can be reversed.
- Destination/source conflicts require confirmation.
- Partial undo reports completed and failed entries and remains in
  history.
- Cancelled or irreversible operations never advertise Undo.
