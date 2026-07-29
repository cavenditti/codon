---
id: TASK:phase-24/fm-adaptive-enrichment
type: task
status: accepted
version: 0.1.0
summary: >
  Prioritize basic listing paint ahead of visible metadata, git, and
  preview enrichment.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-io-scheduler#c-adaptive-enrichment
blocked_by:
  - TASK:phase-24/fm-visible-child-counts
  - TASK:phase-24/fm-git-status-scheduler
---

# Adaptive enrichment scheduler

Define basic-listing, visible-metadata, git, selected-preview, and
adjacent-prefetch priority classes with cancellation on generation
changes.

## Acceptance

- First listing paint waits only for names/basic entry types.
- Explicit navigation preempts adjacent prefetch.
- Queue and completion timing are visible in diagnostics traces.
