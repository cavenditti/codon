---
id: TASK:phase-2/swap-files
type: task
status: accepted
version: 0.1.0
summary: >
  Unsaved-buffer recovery via Zed's EditorDb (ProjectSettings::session.restore_unsaved_buffers default true).
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-swap-files
---

# Unsaved buffer recovery

No filesystem .swp sidecar added — the SQLite-based mechanism already handles graceful and forced-kill recovery.
