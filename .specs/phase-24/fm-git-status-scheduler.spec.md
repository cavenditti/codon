---
id: TASK:phase-24/fm-git-status-scheduler
type: task
status: accepted
version: 0.1.0
summary: >
  Debounce, deduplicate, cache, and narrowly invalidate file-manager
  git-status collection per repository.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-io-scheduler#c-git-status-scheduler
blocked_by: []
---

# FM git-status scheduler

Maintain one in-flight job and a short-lived result per repository.
Merge callers waiting on the same revision, debounce focus bursts, and
invalidate from repository/worktree events or successful FM mutations.

## Acceptance

- Repeated focus/reload/navigation bursts launch one git command.
- Warm navigation within a repository reuses cached status.
- Ignored-file enrichment is requested only when the active visibility
  policy needs it.
