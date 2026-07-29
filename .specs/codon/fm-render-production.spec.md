---
id: REQ:codon/fm-render-production
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Make the phase-17 custom file-manager renderer measurable, cache
  correct, visually equivalent to the legacy path, independent of
  listing size on repaint, and safe to enable by default.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-24]
---

# Production file-manager renderer

:::{requirement id="fm-render-production" level="MUST"}
The system MUST:

- {#c-real-frame-trace} record timing only after the rendered frame has
  completed, including render-tree construction, custom-column
  prepaint, custom-column paint, residual draw/presentation time,
  rows painted, and cache hits/misses. The report MUST pair each
  keypress with the next completed frame and ship a reproducible replay.
- {#c-stable-snapshots} retain immutable listing/render snapshots and
  shaped-line caches across frames so repaint performs no full-list
  clone, conversion, aggregation, or allocation. Work per repaint MUST
  scale with visible rows.
- {#c-cache-correctness} invalidate cached rows whenever any painted
  input changes, including metadata label, git decoration, filetype
  styling, symlink state, icon, mark/selection state, line mode, font,
  or theme.
- {#c-visual-parity} preserve the legacy renderer's git glyph and
  filename colors, filetype colors, fallback icons, symlink indicator,
  mouse selection, jump targets, clipping, and responsive metadata
  behavior.
- {#c-default-fast-path} enable the custom renderer by default only
  after automated parity tests pass and the 500-entry replay meets
  completed-frame p95 ≤ 5 ms, keypress-to-present p95 ≤ 16 ms,
  row-cache hit rate ≥ 95%, and shaped-line hit rate ≥ 90%. A reversible
  legacy-render preference MUST remain available.
:::
