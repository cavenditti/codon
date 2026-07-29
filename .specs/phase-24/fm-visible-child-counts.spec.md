---
id: TASK:phase-24/fm-visible-child-counts
type: task
status: accepted
version: 0.1.0
summary: >
  Replace eager all-directory child counting with cached visible-range
  enrichment under bounded concurrency.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-io-scheduler#c-visible-child-counts
blocked_by:
  - TASK:phase-24/fm-render-stable-snapshot
---

# Visible-only child counts

Expose the visible ranges from both render paths, request counts only
for Size mode rows in those ranges plus a small lookahead, and cache
results by path/directory mtime.

## Acceptance

- Entering a directory with 1,000 subdirectories reads no more than the
  visible window plus configured lookahead before first interaction.
- Concurrent count jobs are bounded and superseded listings cancel.
