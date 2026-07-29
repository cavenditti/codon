---
id: TASK:phase-24/fm-render-parity
type: task
status: accepted
version: 0.1.0
summary: >
  Restore legacy row visuals and interaction behavior in the custom
  row/column renderer.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-render-production#c-visual-parity
blocked_by:
  - TASK:phase-24/fm-render-cache-correctness
---

# Custom-render visual and interaction parity

Paint git glyphs/colors, filetype colors, fallback icons, and symlink
indicators; provide view-level pointer hit testing and jump targets; and
match legacy clipping and responsive metadata rules.

## Acceptance

- Golden/manual parity matrix covers clean, dirty, marked, selected,
  symlink, executable, hidden, missing-icon, and narrow layouts.
- Mouse selection and jump hints land on the same rows as the legacy
  renderer.
- Keyboard navigation remains independent of per-row handlers.
