---
id: TASK:phase-13/status-bar-layout-rewire
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the flat add_left_item / add_right_item block in
  apps/codon/src/zed.rs with the three-zone registration matching
  the modeline diagram in REQ:codon/status-bar.
owners: [carlo]
progress: done
refines:
  - REQ:codon/status-bar#c-three-zones
  - REQ:codon/status-bar#c-centre-pane-context
  - REQ:codon/status-bar#c-right-meta-and-dynamic
aspects: [zone-registration, centre-items, right-items]
---

# Rewire the codon status-bar registration into three zones

## What changes

`apps/codon/src/zed.rs:581-602` currently registers seventeen items
via a mix of `add_left_item` / `add_right_item` calls in arbitrary
order (the block even includes a `project_info` `add_left_item` call
between two `add_right_item` calls). After
[TASK:phase-13/status-bar-center-zone-api](spec:TASK:phase-13/status-bar-center-zone-api)
lands, that block is rewritten to use three explicit zones:

```text
LEFT     mode  ·  session  ·  windows
CENTER   git_branch  ·  pane_context  ·  active_buffer_language  ·  cursor_position
RIGHT    activity_indicator  ·  diagnostic_summary  ·  lsp_button
         ·  merge_conflict_indicator  ·  edit_prediction_ui
         ·  active_toolchain_language  ·  active_buffer_encoding
         ·  line_ending_indicator  ·  project_info  ·  image_info
```

Right-zone registration order is **rightmost-first** in the source —
`status_bar.right_items` is rendered with `.rev()` in
`render_right_tools` today, so the call that goes in first sits
rightmost on screen. The list above is screen order; the
`add_right_item` call order in code is the same list reversed.

`search_button` is *not* registered (see
[TASK:phase-13/status-bar-search-button-removal](spec:TASK:phase-13/status-bar-search-button-removal)).
`active_file_name` is replaced by the new pane-context label (see
[TASK:phase-13/status-bar-pane-context-item](spec:TASK:phase-13/status-bar-pane-context-item)).
`git_branch` is the new branch indicator (see
[TASK:phase-13/status-bar-git-branch-item](spec:TASK:phase-13/status-bar-git-branch-item)).

## Approach

1. Wait for `status-bar-center-zone-api` and the two new-item tasks
   to be in-progress or done.
2. In `apps/codon/src/zed.rs`, replace the existing `status_bar.update`
   block with:
   ```rust
   workspace.status_bar().update(cx, |status_bar, cx| {
       // Left — global state; protected from collapse.
       status_bar.add_left_item(vim_mode_indicator, window, cx);
       status_bar.add_left_item(session_indicator, window, cx);
       status_bar.add_left_item(windows_indicator, window, cx);

       // Centre — focused pane context.
       status_bar.add_center_item(git_branch_indicator, window, cx);
       status_bar.add_center_item(pane_context_label, window, cx);
       status_bar.add_center_item(active_buffer_language, window, cx);
       status_bar.add_center_item(cursor_position, window, cx);

       // Right — meta and dynamic messaging; rightmost-first.
       status_bar.add_right_item(activity_indicator, window, cx);
       status_bar.add_right_item(diagnostic_summary, window, cx);
       status_bar.add_right_item(lsp_button, window, cx);
       status_bar.add_right_item(merge_conflict_indicator, window, cx);
       status_bar.add_right_item(edit_prediction_ui, window, cx);
       status_bar.add_right_item(active_toolchain_language, window, cx);
       status_bar.add_right_item(active_buffer_encoding, window, cx);
       status_bar.add_right_item(line_ending_indicator, window, cx);
       status_bar.add_right_item(project_info, window, cx);
       status_bar.add_right_item(image_info, window, cx);
   });
   ```
3. Delete the unused `search_button` local and its dependency on
   the `search` crate at the registration site.
4. Delete the `active_file_name` local; the new pane-context label
   subsumes it.

## Non-goals

- No change to which items exist — only their position. Removing
  items besides `search_button` and `active_file_name` is out of
  scope.
- No theme / spacing changes. Visual polish lives in a follow-up
  task once the zones are wired and observable.

## Files touched

- `apps/codon/src/zed.rs` — the `status_bar.update` block and its
  surrounding `let …` bindings.

## Verification

- `cargo run -p codon` launches with the new layout: mode and
  windows hard-left, branch/file/language/cursor centred, build
  progress rightmost.
- Resizing the window narrower clips in the order specified by
  [REQ:codon/status-bar#c-collapse-policy](spec:REQ:codon/status-bar#c-collapse-policy)
  — verified visually, no automated test.
- `mode`, `session`, and `windows` items remain fully visible at
  the narrowest window the OS allows.
