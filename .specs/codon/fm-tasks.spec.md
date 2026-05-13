---
id: REQ:codon/fm-tasks
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Long-running fs operations (paste, bulk delete, bulk rename, archive
  preview decode) surface as expandable notifications in the existing
  notification system — with live progress and a cancel control.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-8]
---

# File manager tasks

Yazi's `w` task panel is a dedicated layer. Codon already has a
notification system (`MessageNotification`, used by hold-quit and
others). Reusing it keeps the user's mental model unified: every
async thing codon does shows up in the same place.

:::{requirement id="fm-tasks" level="SHOULD"}
The file manager SHOULD surface long-running operations as
notifications:

- {#c-task-as-notifications} every fs op that touches more than a
  threshold number of entries (start at: ≥ 3 entries OR estimated
  byte volume ≥ 50 MB) emits a notification on start, replaces it
  with a progress notification while running (count + ETA), and
  resolves to a success / failure notification on completion. Short
  operations stay invisible (no UI thrash for a 1-file paste).
- {#c-task-cancel} the live progress notification carries an `x`
  action that requests cancellation. Cancellation is cooperative —
  each `fs::Fs` future checks a `CancellationToken` between
  per-entry chunks. Partial state is preserved (already-renamed
  files don't roll back) and surfaced in the resolution
  notification ("cancelled after 12 of 50").
- {#c-task-history} `w` opens a modal listing the last 50 fs tasks
  (active + recent), with start time, duration, status, and a
  re-emit-as-notification action for ones whose notification was
  dismissed. Backing store is in-memory only; cleared on quit.
:::
