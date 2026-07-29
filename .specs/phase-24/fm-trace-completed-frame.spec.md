---
id: TASK:phase-24/fm-trace-completed-frame
type: task
status: accepted
version: 0.1.0
summary: >
  Replace the provisional render-closure trace with completed-frame
  timing and real custom-column/cache counters.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/fm-render-production#c-real-frame-trace
blocked_by: []
---

# Completed-frame FM trace

Schedule one post-frame callback from the FM render boundary, aggregate
custom-column prepaint/paint durations and cache/row counters into that
frame, and derive residual draw/presentation time from the completed
wall time. Remove the provisional zero-valued frame event.

## Acceptance

- `frame_painted.at_ms` is captured after GPUI finishes the frame.
- `prepaint_ms`, `paint_ms`, `draw_ms`, `rows_painted`,
  `cache_hits`, and `cache_misses` contain measured values.
- Multiple FM render passes before one frame produce one aggregate
  event.
- The JSONL/report tests cover completed-frame and cache metrics.
- A scripted 500-entry replay is checked into `scripts/`.
