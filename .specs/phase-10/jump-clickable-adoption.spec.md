---
id: TASK:phase-10/jump-clickable-adoption
type: task
status: accepted
version: 0.0.1
summary: >
  Wrap user-visible interactive surfaces — workspace tabs, dock
  toggles, status bar items, panel headers (git, agent, project),
  notifications — with `.jump_target(...)`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/jump-hints#c-clickable-adoption
aspects: [tabs, status-bar, dock-toggles, panel-headers, notifications]
---

# JumpClickable adoption

## What ships

Per surface, a 1-line `.jump_target(...)` on the existing
`Button` / focusable element. The `on_click` closure passed to
`jump_target` mirrors the element's existing `on_click`.

Surfaces (each is a vendored Zed file):

- `vendor/zed/crates/workspace/src/pane.rs` — tab strip.
- `vendor/zed/crates/workspace/src/dock.rs` — left/right/bottom
  dock toggles.
- `vendor/zed/crates/workspace/src/status_bar.rs` — every
  registered status bar item.
- `vendor/zed/crates/title_bar/src/title_bar.rs` — title bar
  buttons.
- `vendor/zed/crates/git_ui/src/git_panel.rs` — file rows in
  the changes list.
- `vendor/zed/crates/agent_ui/src/...` — agent panel header
  buttons + message-action buttons.
- `vendor/zed/crates/project_panel/src/project_panel.rs` —
  one candidate per visible entry.
- `vendor/zed/crates/notifications/src/notifications.rs` —
  notification dismiss button + any inline-action button.

Total: ~8 files × 1-3 sites each, ~24 sites total. Each site
is mechanical: find the `.on_click(...)` call, follow with
`.jump_target(...)` cloning the same closure. Where the existing
on_click closure captures `cx.listener`, the `jump_target` arg
uses the same listener pattern.

The wrapper is a no-op at paint time when no overlay is open:
`ClickableRegistry::push` is a single `Vec::push` behind a
`thread_local!` `RefCell`. Cost in steady state is one Arc
clone per painted button per frame — well within budget.

## Verification

- `cmd-k j` from an empty workspace: visible tabs, dock toggles,
  and status bar items all get chips. Two-key selection clicks
  the element (focuses the tab / toggles the dock / dispatches
  the status item's action).
- Open git panel + agent panel: their header buttons hint
  alongside everything else.
- Project panel: every visible entry hints; selecting one focuses
  the panel and selects the entry.

## Where it slots in

- ~8 vendored Zed files; one commit per cluster (workspace,
  panels, notifications) to keep diff review-able.
- Vendor/zed submodule bump in the outer commit.

## Out of scope

- Adopting `.jump_target` inside Picker rows / modal lists — the
  overlay would conflict with the modal's own keystroke handling.
  Pickers already have their own j/k nav + fuzzy filter; adding
  hint-mode there is low-value.
