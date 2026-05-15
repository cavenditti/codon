---
id: REQ:codon/status-bar
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  The bottom status bar is a three-zone modeline — left holds
  protected global state (mode, session, windows), centre describes
  the focused pane, right carries meta and dynamic messaging.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-13]
---

# Status bar

## Context

Codon is a modal multiplexer; the status bar is the user's permanent
read-out of *where they are in the shell*. Today the bar is
inherited Zed plumbing: two flat vecs of `StatusItemView`s rendered
as a single `justify_between` row in
`vendor/zed/crates/workspace/src/status_bar.rs`, populated in
`apps/codon/src/zed.rs:581` in arbitrary order. The codon-identity
signals (pane mode, active session, window list) sit next to
encoding pickers and image dimensions; under width pressure the bar
clips arbitrary middle indicators rather than the least-useful ones.

This requirement turns the bar into a deliberate three-zone modeline
with an explicit collapse order. Mode and window indicators become
load-bearing — they MUST remain visible even when everything else
has been clipped away — because they are how the user navigates the
multiplexer at all.

:::{requirement id="status-bar" level="MUST"}
The system MUST provide a three-zone status bar with:

- {#c-three-zones} a left zone, a centre zone, and a right zone, each
  a separate flex container in the `StatusBar` render. The left zone
  shrinks last; the centre zone takes the remaining flex space; the
  right zone is `flex_shrink_0`. The bar MUST NOT render as a single
  flat row; the three zones are independent collections of status
  items registered via distinct constructors.

- {#c-left-protected} the left zone contains, in order, the pane
  mode indicator, the active session name, and the windows
  indicator. These three items MUST NEVER be hidden, truncated, or
  reordered under any width condition. If the bar is too narrow to
  fit them alongside the centre and right zones, the centre zone
  collapses first and the right zone collapses second; the left
  zone is the last to lose pixels and even then individual items
  within it remain fully readable. Mode is the leftmost slot — it
  is the highest-frequency signal in a modal shell and the eye
  lands there first.

- {#c-centre-pane-context} the centre zone describes the currently
  focused pane. It contains, in order: git-branch indicator,
  pane-context label (file path for editor panes, cwd for terminal
  and file-manager panes, agent verb for agent panes), active
  language, cursor position (`line:col`). The centre zone is
  pane-aware: switching focus to a non-editor pane swaps the file
  segment for the appropriate alternative without re-laying-out the
  bar.

- {#c-right-meta-and-dynamic} the right zone carries everything
  else, with `activity_indicator` anchored rightmost so background
  build/format/LSP progress messages read at the edge of the eye.
  Order from rightmost inward: `activity_indicator`,
  `diagnostic_summary`, `lsp_button`, `merge_conflict_indicator`,
  `edit_prediction_ui`, `active_toolchain_language`,
  `active_buffer_encoding`, `line_ending_indicator`, `project_info`,
  `image_info`. Right-zone items remain visible until the collapse
  policy clips them in order (see `#c-collapse-policy`).

- {#c-collapse-policy} when the bar is narrower than the sum of its
  preferred widths it MUST clip in this order, never deviating:
  1. centre file/cwd/verb segment switches to middle-ellipsis
     truncation (reusing `active_file_name`'s existing behaviour);
  2. centre language segment drops;
  3. centre cursor segment drops;
  4. centre git-branch segment drops;
  5. right zone clips from the leftmost end (i.e. `image_info`
     first, then `project_info`, `line_ending_indicator`,
     `active_buffer_encoding`, `active_toolchain_language`,
     `edit_prediction_ui`, `merge_conflict_indicator`, `lsp_button`,
     `diagnostic_summary`, `activity_indicator`).
  The left zone is never clipped. This policy is the operational
  expression of `#c-left-protected`: the protection is enforced by
  the render path, not by a separate guard.

- {#c-no-search-button} `search_button` (the magnifying-glass
  control inherited from Zed's `search::search_status_button`) is
  removed from the status bar entirely. Search is a verb, not
  status; it is reachable through the keymap and the command
  palette only. This clause is satisfied by the absence of any
  `add_left_item` / `add_center_item` / `add_right_item` call that
  registers a `SearchButton` in `apps/codon/src/zed.rs`.

- {#c-vendored-zone-api} `vendor/zed/crates/workspace/src/status_bar.rs`
  MUST grow a `center_items: Vec<Box<dyn StatusItemViewHandle>>`
  field, an `add_center_item` constructor mirroring `add_left_item`,
  and a three-cell render path. `item_of_type`, `position_of_item`,
  `insert_item_after`, `remove_item_at`, and `update_active_pane_item`
  MUST walk all three vecs. The change is additive — no existing
  Zed consumer calls `add_center_item`, so upstream Zed consumers
  see no behavioural change.

- {#c-git-branch-item} a new status item (`GitBranchIndicator`)
  shows the active pane's repository branch, falling back to the
  project's primary repository when the active pane has no repo of
  its own. Click opens the existing `git_ui` branch picker. The
  item lives in the centre zone, leftmost.

- {#c-pane-context-item} a new status item (`PaneContextLabel`)
  replaces `active_file_name` in the centre zone. It renders:
  - the file path (with middle-ellipsis) for editor panes;
  - `term: <cwd>` for terminal panes;
  - `fm: <cwd>` for file-manager panes;
  - `agent: <verb>` for agent panes (e.g. `agent: Explain`).
  The item subscribes to the `codon-mode` pane-kind tracker so
  switching focus updates the label without a re-layout.
:::

## Implementation

The vendored Zed change (`#c-vendored-zone-api`) lands as a
submodule commit on the `codon` branch with a pointer bump in the
outer repo. The render path uses a flex layout with `min_w_0` on
the left zone (allowing the centre to take precedence visually),
`flex_1` on the centre, and `flex_shrink_0` on the right;
truncation order from `#c-collapse-policy` is enforced by item
ordering within each zone plus `truncate` modifiers on the centre
file/cwd segment.

The two new status items (`GitBranchIndicator`, `PaneContextLabel`)
live in a small codon-side crate or alongside the existing items in
`codon-session` — implementation choice deferred to the task. Both
implement `workspace::StatusItemView` and are registered in
`apps/codon/src/zed.rs:581` alongside the reordered existing items.
