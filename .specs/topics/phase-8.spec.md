---
id: TOPIC:topics/phase-8
type: topic
status: draft
version: 0.0.1
summary: >
  Yazi-feature-parity for the file manager, wave 3 — symlinks, bulk
  $EDITOR ops, tasks-as-notifications, trash recovery.
owners: [carlo]
---

# Phase 8 — File-manager parity, wave 3

The remainder of the yazi surface, gated on phase-6 / phase-7 patterns
(notifications, marked-set verbs, opener model).

Refining requirements:

- [REQ:codon/fm-symlinks](spec:REQ:codon/fm-symlinks) — make
  symlink (`ln`), make hardlink (`Ln`), follow symlink on Enter.
- [REQ:codon/fm-bulk-editor](spec:REQ:codon/fm-bulk-editor) — bulk
  rename via $EDITOR (`cw`) complementing the phase-5 `R` pattern,
  bulk chmod (`cm`).
- [REQ:codon/fm-tasks](spec:REQ:codon/fm-tasks) — long-running fs ops
  surface as expandable notifications (shared with the existing
  notification system) with progress + cancel.
- [REQ:codon/fm-trash](spec:REQ:codon/fm-trash) — `T` lists the OS
  trash, Enter restores, `X` permanently deletes. Uses the `trash`
  crate codon already pulls in for `D`.
