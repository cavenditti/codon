---
id: TOPIC:topics/phase-17
type: topic
status: draft
version: 0.0.1
summary: >
  App-wide performance phase — collapse GPUI's div pipeline for the
  file-manager view into custom Elements (row + column + glyph
  caches + deferred editor preview + dirty-rect repaint +
  measurement harness), and cut the synchronous cost of
  window/session switching by skipping LayoutSnapshot capture on
  cache-hit paths, deferring registry persistence, fixing O(N×M)
  pane-set merge inside Zed's restore_center_root, eliding
  unconditional pane-notify, and deferring overview-modal capture.
owners: [carlo]
---

# Phase 17 — App-wide performance

After Phase 15's async file-manager work and Phase 16's UX
coverage, the remaining responsiveness gaps live in two places:

1. **File-manager redraws.** With the data-side work (read_dir,
   git status, dir cache, label memo) all moved off-thread, the
   30 ms-per-frame baseline that still makes directory-enter
   feel slower than `yazi` lives inside GPUI's `Div` pipeline —
   `Interactivity::paint`, `with_text_style`, `with_image_cache`,
   `with_optional_element_state`, taffy flexbox, and per-frame
   text reshaping. None of that is needed for what FM rows
   actually do. Custom GPUI Elements (the pattern
   `editor::EditorElement` already uses) plus per-row glyph
   caches collapse the work to a translate-and-emit, and the
   preview column defers its real `EditorElement` until selection
   dwells.

2. **Window- and session-switching.** Every switch synchronously
   walks the outgoing pane tree (`swap::capture` →
   `LayoutSnapshot`), upserts the in-memory registry, queues a
   background JSON persist task, and pushes through Zed's
   `restore_center_root` which does an O(N×M) pane-set merge
   *and* invalidates every pane's render cache. The runtime
   cache exists exactly to avoid that work — most of the
   synchronous steps can be skipped on the cache-hit path.

Both clusters get the same render-trace measurement harness, so
the acceptance gates are objective.

## REQs

- [`REQ:codon/fm-render`](spec:.specs/codon/fm-render.spec.md) —
  custom Elements + glyph caches + deferred preview editor +
  dirty-rect repaint + frame-budget harness. Acceptance: ≤ 5 ms
  / FM redraw at p95, ≤ 3 ms typical with cache hits.
- [`REQ:codon/perf-switch`](spec:.specs/codon/perf-switch.spec.md)
  — skip LayoutSnapshot capture on cache hit, defer registry
  persistence, HashSet pane-set merge in `restore_center_root`,
  notify only newly-attached panes, defer overview-modal
  capture, and the switch-timing extension to the trace
  harness. Acceptance: ≤ 8 ms p95 for an intra-session window
  switch on the cache-hit path, ≤ 12 ms for a session switch.

## Tasks

```
TASK:phase-17/fm-render-custom-row              # row Element bypasses Div
TASK:phase-17/fm-render-custom-column           # column Element bypasses uniform_list
TASK:phase-17/fm-render-shaped-line-cache       # FM-scoped (font, size, text) LRU
TASK:phase-17/fm-render-row-glyph-cache         # cached row PaintGlyph vec
TASK:phase-17/fm-render-defer-editor-preview    # static preview during fast nav
TASK:phase-17/fm-render-dirty-rect              # mark_rows_dirty + partial repaint
TASK:phase-17/fm-render-frame-budget            # JSONL render-trace + acceptance gate

TASK:phase-17/switch-skip-capture-on-cache-hit  # lazy LayoutSnapshot
TASK:phase-17/switch-defer-persist              # debounced registry flush
TASK:phase-17/switch-restore-pane-set-hashmap   # vendored Zed: O(N+M) merge
TASK:phase-17/switch-restore-skip-notify        # vendored Zed: notify only new panes
TASK:phase-17/switch-overview-defer-capture     # overview reads cached snapshot
TASK:phase-17/switch-budget-harness             # SwitchTiming + scripted replay
```

Phase 17 ships when every TASK is `done` and the two acceptance
gates above both report green on the reference Apple-Silicon
device, with `spec lint` at zero errors.
