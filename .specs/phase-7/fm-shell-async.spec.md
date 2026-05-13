---
id: TASK:phase-7/fm-shell-async
type: task
status: accepted
version: 0.0.1
summary: >
  `;` runs a shell command non-blocking. Control returns to the FM
  immediately; output lands in the same terminal pane as `!`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-shell-exec#c-shell-async
---

# File-manager shell exec — async (`;`)

## What ships

`;` opens an Insert-mode prompt (same substitution surface as
`!`). Enter spawns the command in the terminal pane chosen by
TASK:phase-7/fm-shell-terminal-reuse without overlaying the FM —
the user can keep navigating. No exit subscription, no toast on
failure (TASK:phase-7/fm-shell-stderr-toast skips async by design).

Effectively `!` minus the overlay + minus the toast.

## Where it slots in

Same prompt + terminal-pane spawn path as TASK:phase-7/fm-shell-blocking;
the async-vs-blocking branch is a single boolean. ~80 LOC on top
of that task.
