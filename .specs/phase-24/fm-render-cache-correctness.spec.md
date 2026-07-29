---
id: TASK:phase-24/fm-render-cache-correctness
type: task
status: accepted
version: 0.1.0
summary: >
  Make the row-glyph cache key and invalidation hooks cover every
  painted input, including async enrichment and theme changes.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/fm-render-production#c-cache-correctness
blocked_by: []
---

# Correct render-cache invalidation

Introduce an explicit entry visual revision/key and a theme/font
generation. Increment or rebuild it on child-count, git, icon,
filetype-theme, mark, line-mode, and filesystem updates.

## Acceptance

- Unit tests demonstrate misses after each painted input changes.
- Theme and file-manager-theme hot reload clear all FM render caches.
- Async child-count/git fills never leave stale visible labels.
