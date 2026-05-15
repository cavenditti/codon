---
id: TASK:phase-13/status-bar-git-branch-item
type: task
status: accepted
version: 0.0.1
summary: >
  New GitBranchIndicator status item — shows the active pane's
  repo branch, falls back to project primary repo, click opens
  git_ui's branch picker.
owners: [carlo]
progress: done
refines:
  - REQ:codon/status-bar#c-git-branch-item
---

# Status-bar git-branch indicator

## What changes

Codon's status bar has no git-branch readout today. The active
branch is a high-signal pane-context fact (especially for editor
panes whose buffer lives in a repo), so it earns the leftmost slot
of the centre zone.

The new item:

- implements `workspace::StatusItemView`;
- reads the active pane item's repository when available
  (`ItemHandle::project_path` → `Project::git_store` → repository
  lookup);
- falls back to the project's primary repository (first entry in
  `Project::worktrees().filter_map(|w| w.repository())`) when the
  active pane has no repo of its own — terminal, file-manager,
  and agent panes get the project-wide branch this way;
- renders `branch_name` (or `(no branch)` for detached HEAD);
- click dispatches the existing `git_ui` branch picker action
  (the action that `git_ui::repository_selector` triggers today
  for its own UI).

## Approach

1. Pick a home: extend `crates/codon-session/` with a
   `git_branch_indicator.rs` module, OR create a small
   `crates/codon-git-status` crate. Prefer the former — the
   existing crate already owns status-bar items (`SessionStatusItem`,
   `WindowsStatusItem`) and adding a third keeps the related code
   together.
2. Implement the item:
   ```rust
   pub struct GitBranchIndicator { /* WeakEntity<Workspace>, current branch SharedString */ }

   impl StatusItemView for GitBranchIndicator { … }
   impl Render for GitBranchIndicator { … }
   ```
3. Subscribe to repository updates so the indicator refreshes when
   the user checks out a different branch externally.
4. Wire the click handler to the same action `git_ui` exposes for
   its branch picker (`git_ui::branches::OpenBranchPicker` or
   equivalent — confirm the action name in
   `vendor/zed/crates/git_ui/`).
5. Register the item in the centre zone (via
   [TASK:phase-13/status-bar-layout-rewire](spec:TASK:phase-13/status-bar-layout-rewire)).

## Non-goals

- No ahead/behind indicator. That belongs in a follow-up clause if
  the user asks for it.
- No write actions (commit, push, pull) wired from the indicator.
  Click opens the branch picker only.
- No custom branch picker. Reuse the existing `git_ui` one.

## Files touched

- `crates/codon-session/src/git_branch_indicator.rs` (new).
- `crates/codon-session/src/lib.rs` (export the new item).
- `apps/codon/src/zed.rs` — construct and register the item (the
  registration call itself lives in `status-bar-layout-rewire`).
- `crates/codon-session/Cargo.toml` — add `git_ui` and `git` as
  dependencies if not already present.

## Verification

- `cargo run -p codon` shows the active branch as the leftmost
  centre-zone segment in an editor pane inside a git repo.
- Focusing a terminal pane keeps the indicator pinned to the
  project's primary repo (verified by switching focus back and
  forth).
- Clicking the indicator opens the same branch picker reachable
  through the `git_ui` keybinding.
- Switching branches externally (in a shell) updates the
  indicator within one event-loop tick.
