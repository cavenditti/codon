---
id: TASK:phase-24/fm-render-default
type: task
status: accepted
version: 0.1.0
summary: >
  Enable the custom renderer by default after parity and completed-frame
  performance gates pass.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-render-production#c-default-fast-path
blocked_by:
  - TASK:phase-24/fm-trace-completed-frame
  - TASK:phase-24/fm-render-stable-snapshot
  - TASK:phase-24/fm-render-cache-correctness
  - TASK:phase-24/fm-render-parity
---

# Default fast render path

Flip the serde and runtime default to the custom renderer only after
the automated parity suite and reference replay pass. Preserve
`custom_render = false` as an escape hatch and document it.
