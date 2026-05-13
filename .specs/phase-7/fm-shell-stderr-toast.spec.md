---
id: TASK:phase-7/fm-shell-stderr-toast
type: task
status: accepted
version: 0.0.1
summary: >
  Non-zero `!` (blocking) exit surfaces captured stderr via
  surface_error. Async `;` does not — the user moved on.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-shell-exec#c-shell-stderr-toast
---

# File-manager shell-exec stderr toast

## What ships

For `!` (blocking) only: capture the command's last 8 lines of
stderr (ring buffer) and pass them through
`FileManager::surface_error` on non-zero exit. The toast title is
the first 60 chars of the command line; the body is the stderr
tail.

`;` (async) skips this — the user already left context, and the
output is already visible in the terminal pane.

## Where it slots in

The blocking-exec exit-event subscription
(TASK:phase-7/fm-shell-blocking). ~40 LOC.
