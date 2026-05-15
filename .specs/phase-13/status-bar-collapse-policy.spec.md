---
id: TASK:phase-13/status-bar-collapse-policy
type: task
status: accepted
version: 0.0.1
summary: >
  Enforce the centre and right zone clip order from
  REQ:codon/status-bar#c-collapse-policy via render-time truncation
  on the file/cwd segment and ordered item rendering.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/status-bar#c-collapse-policy
---

# Status-bar collapse policy

## What changes

[REQ:codon/status-bar#c-collapse-policy](spec:REQ:codon/status-bar#c-collapse-policy)
defines the order in which the bar clips under width pressure:

1. Centre file/cwd/verb segment → middle-ellipsis truncation.
2. Centre language segment → drop.
3. Centre cursor segment → drop.
4. Centre git-branch segment → drop.
5. Right zone clips from its leftmost end (`image_info` first,
   `activity_indicator` last).

The flex layout from
[TASK:phase-13/status-bar-left-protected](spec:TASK:phase-13/status-bar-left-protected)
handles steps 4 → 5 → centre-vs-right priority *for free* through
`flex_shrink_0` on the left zone and `flex_1 min_w_0` on the centre.
This task adds the per-item behaviour that produces the right
*order* within the zones.

## Approach

1. **Centre file/cwd segment** — `PaneContextLabel` (added by
   `status-bar-pane-context-item`) already uses middle-ellipsis
   for file paths. Extend the same truncation to the `cwd` and
   `verb` variants so step 1 of the policy applies uniformly.
2. **Centre item drop order (steps 2-4)** — registering items in
   the order `[git_branch, pane_context, language, cursor]` is
   *not* enough on its own; flex collapses the trailing items
   first by default, which would drop the cursor then language
   then file. To get step 2 (language) before step 3 (cursor),
   place language *after* cursor in source order and rely on the
   bar reading right-to-left when clipping, OR use explicit
   `flex_shrink` priorities (`flex_shrink: 1` on language,
   `flex_shrink: 2` on cursor, etc.). Pick the explicit-priority
   path — it survives later reordering.
3. **Right zone clip from leftmost end** — `render_right_tools`
   already renders `right_items.iter().rev()`, so the
   first-registered item is rightmost. Adding `flex_shrink` to
   the right container (per `status-bar-left-protected`) means
   the leftmost-rendered items (last-registered) collapse first,
   which matches step 5 if registration follows the order
   defined in
   [TASK:phase-13/status-bar-layout-rewire](spec:TASK:phase-13/status-bar-layout-rewire).
4. Verify the order with a manual narrowing pass; document any
   deviations in a follow-up `#c-collapse-policy` clause edit.

## Non-goals

- No animations. Items pop in / out abruptly at flex breakpoints.
- No tooltips for hidden items. If you can't read it, find it in
  the command palette or settings — the bar is status, not a UI.
- No per-zone gap tweaking. The default `gap_1` stays.

## Files touched

- `vendor/zed/crates/workspace/src/status_bar.rs` — flex-shrink
  priorities on centre and right cells.
- Possibly `crates/codon-session/src/pane_context_label.rs` (or
  wherever `status-bar-pane-context-item` lands) — uniform
  middle-ellipsis truncation across file / cwd / verb variants.

## Verification

- Manual: drag a codon window from wide to narrow continuously;
  observe items disappear in the exact order specified by
  [REQ:codon/status-bar#c-collapse-policy](spec:REQ:codon/status-bar#c-collapse-policy).
- Manual: at the narrowest viable width only the left zone
  remains visible.
