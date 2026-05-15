---
id: TASK:phase-11/window-close-confirm
type: task
status: accepted
version: 0.1.0
summary: >
  WindowClose prompts before discarding panes that contain dirty
  items.
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-safe-close-confirm
categorized_under: [TOPIC:topics/phase-11]
---

# Confirm on close when dirty

## What ships

`handle_window_close` is augmented: before removing the window, walk
every pane in the active center group and collect items that report
`is_dirty()`. If any are found, show a workspace prompt of the form

> "Save n unsaved item(s) in this window?"
> [Save] [Discard] [Cancel]

reusing `Workspace::prompt_to_save_paths_for_buffers` or the same
`save_intent: AskAll` path that the existing `CloseActiveItem` uses
through `Pane::close_active_item`. The window is only removed after
the prompt resolves; Cancel aborts the close, Save flushes then
removes, Discard removes immediately.

Clean windows close without confirmation, matching today's behavior.

The same prompt logic does NOT live on `SafeCloseActiveItem` step 3
(which delegates to `WindowClose`) — that branch already inherits
the prompt because it dispatches through the same handler.

## Why this shape

Closing a window today is silent data loss for dirty buffers — the
swap path captures the layout but the items don't get a "do you
want to save?" prompt because the pane group is destroyed by the
`replace_center_*` machinery. The cheapest fix is to gate the
destructive call behind the same prompt the rest of Zed uses.

Effort: medium. The prompt API in `Workspace` returns an async
`Task<bool>`, so the handler becomes async — wire it via
`cx.spawn`.
