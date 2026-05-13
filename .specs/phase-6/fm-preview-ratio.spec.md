---
id: TASK:phase-6/fm-preview-ratio
type: task
status: accepted
version: 0.0.1
summary: >
  `<` shrinks and `>` grows the preview column. Floor 10%, ceiling
  80%; persisted across launches.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-sort-display#c-preview-ratio
---

# File-manager preview ratio

## What ships

A `preview_fraction: f32` field on `FileManager` (default
`0.333` — matching today's `1/3` width). `<` and `>` step it by
`0.05` (5% of total). Clamped to `[0.10, 0.80]`. Persisted via
codon-config.

The middle and parent columns share the remaining width
proportionally — keep the parent at its current `1/4` ratio when
preview is at default; scale parent down proportionally as preview
grows so the middle column never collapses past a minimum.

## Where it slots in

- Field on FM state.
- [`crates/file-manager/src/view.rs`](spec:src:crates/file-manager/src/view.rs)
  render() lays out the three columns — replace the hard-coded
  width ratios with the runtime values.
- ~60 LOC.
