---
id: TASK:phase-13/status-bar-left-protected
type: task
status: accepted
version: 0.0.1
summary: >
  Make the left status-bar zone non-clippable — mode, session, and
  windows indicators remain fully visible even when centre and
  right zones have collapsed entirely.
owners: [carlo]
progress: done
refines:
  - REQ:codon/status-bar#c-left-protected
---

# Protect the left zone from width-pressure collapse

## What changes

The three-cell render path introduced by
[TASK:phase-13/status-bar-center-zone-api](spec:TASK:phase-13/status-bar-center-zone-api)
makes the left zone `min_w_0` + `flex_shrink` by default — which
means a narrow window could still eat into mode / session / windows.
[REQ:codon/status-bar#c-left-protected](spec:REQ:codon/status-bar#c-left-protected)
forbids that. This task tightens the render so:

- the left cell is `flex_shrink_0` (never gives up pixels);
- the centre cell remains `flex_1 min_w_0 overflow_x_hidden` (loses
  pixels first, in the order defined by `#c-collapse-policy`);
- the right cell becomes `flex_shrink` with `overflow_x_hidden` so
  it gives up pixels only after the centre has collapsed.

This is the operational expression of "mode and windows MUST NEVER
hide": the protection is a CSS-equivalent guarantee from the flex
layout, not a runtime predicate.

## Approach

1. Inside the `render` impl in
   `vendor/zed/crates/workspace/src/status_bar.rs` (post
   `status-bar-center-zone-api`):
   - `render_left_tools`: change `min_w_0` to `flex_shrink_0`.
   - `render_center_tools`: keep `flex_1 min_w_0`.
   - `render_right_tools`: keep `flex_shrink_0` on the outer
     container *but* the inner item flexbox uses `flex_shrink` so
     items beyond the centre's flex space collapse from the
     leftmost end (which is the rightmost in source order — see
     `#c-collapse-policy` step 5).
2. Add a comment above `render_left_tools` referencing
   [REQ:codon/status-bar#c-left-protected](spec:REQ:codon/status-bar#c-left-protected)
   so the *why* (a non-obvious load-bearing invariant) survives
   future refactors.

## Non-goals

- No per-item priority API. Protection is zone-level, not item-level.
- No "max width" cap on the left zone. The session name and
  windows tab strip set their own widths; if the user creates a
  comically long session name, it will widen the left zone and
  eat into the centre — by design.

## Files touched

- `vendor/zed/crates/workspace/src/status_bar.rs` (render impl
  only; the field/constructor work lives in
  `status-bar-center-zone-api`).

## Verification

- Manual: launch codon, drag the window narrower until the centre
  zone has fully collapsed (file path gone, language gone, cursor
  gone, branch gone) and the right zone has begun losing items
  from the left end; mode, session name, and windows tab strip
  remain fully readable.
- Manual: create a session with a 60-character name; confirm the
  centre zone collapses to make room without the windows
  indicator clipping.
