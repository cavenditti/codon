---
id: REQ:codon/panes-from-panels
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Every Zed dock-hosted Panel is reachable as a first-class workspace
  pane via a single adapter; dock placement becomes an opt-in transient
  "peek" mode rather than the default.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-12]
---

# Panes from panels

## Context

Codon's modal multiplexer model assumes every surface is a pane in the
workspace tree: focused via pane focus, sized via pane splits,
persisted via `LayoutSnapshot`, navigated with `cmd-k h/j/k/l`,
mode-tracked via `CodonModeTracker`. Zed's dock-hosted views break that
assumption — they live in left / right / bottom docks, carry their own
size and visibility state, and use a `toggle_action` rather than a
pane open.

The seven concrete `impl Panel` types in vendored Zed are:

| Panel | Crate | Current codon status |
|---|---|---|
| `AgentPanel` | `agent_ui` | dock; cross-pane verbs seed it (Phase 3) |
| `ProjectPanel` | `project_panel` | dock; superseded by `file-manager` |
| `OutlinePanel` | `outline_panel` | dock; unused by codon today |
| `TerminalPanel` | `terminal_view` | dock; terminals are panes already |
| `GitPanel` | `git_ui` | dock; modal predicates wired (Phase 4) |
| `DebugPanel` | `debugger_ui` | dock; not yet keymap-bound |
| `CollabPanel` | `collab_ui` | dock; out of scope (single-user fork) |

Two earlier attempts to convert individual panels — agent
([TASK:phase-3/agent-pane-conversion](spec:TASK:phase-3/agent-pane-conversion),
deferred) and git status
([TASK:phase-4/git-status-pane](spec:TASK:phase-4/git-status-pane),
wontdo) — concluded that per-panel rewrites are uneconomic. This REQ
goes one level up: build one adapter, apply it to all of them.

The `Panel` trait
([`vendor/zed/crates/workspace/src/dock.rs`](spec:src:vendor/zed/crates/workspace/src/dock.rs))
has 30+ methods, but only a handful are load-bearing for our case:
`Focusable + Render`, `persistent_name`, `pane()`, `set_active`,
`set_zoomed`. The rest (`position`, `default_size`, `min_size`,
`toggle_action`, `icon`, `icon_label`, `activation_priority`,
`is_zoomed`, `starts_open`, `set_position`, …) only exist because of
the dock host and become no-ops or trivially mappable when the host is
a pane.

## Requirement

:::{requirement id="panes-from-panels" level="MUST"}
The system MUST provide:

- {#c-adapter} a single `PanelItemAdapter<P: Panel>` that wraps any
  Zed `Panel` impl as a `workspace::Item`. The adapter forwards
  `Focusable`, `Render`, and `EventEmitter<PanelEvent>` to the inner
  panel; supplies `Item::tab_content` from the panel's
  `persistent_name` + `icon` + `icon_label`; routes `Item` focus to
  the panel's focus handle; and translates `set_active(true|false)`
  on pane activation. Dock-specific accessors
  (`position`/`default_size`/`min_size`/`toggle_action`/`is_zoomed`/
  `set_position`/`set_zoomed`/`starts_open`/`activation_priority`)
  are *not* required by the adapter — they remain on the panel only
  for callers still using dock placement (peek mode).

- {#c-inventory} a per-panel decision recorded in this REQ for each
  of the seven `impl Panel` types in vendored Zed. Allowed verdicts:
  *convert* (host via adapter as a regular pane), *peek-only* (only
  reachable as a transient dock peek), *drop* (no codon entry point),
  *already-replaced* (a codon-native pane subsumes it). At minimum:
  `AgentPanel` → convert; `GitPanel` → convert; `OutlinePanel` →
  convert; `DebugPanel` → convert; `ProjectPanel` → already-replaced
  by `file-manager`; `TerminalPanel` → drop (terminals are panes
  already, the *Panel* is just the dock host); `CollabPanel` → drop
  (single-user fork).

- {#c-peek-mode} an optional transient "peek" placement. A peek
  mounts the same panel view into a single reusable dock surface
  (one side at a time, not three persistent docks), auto-dismisses
  on focus-loss or `esc`, and never persists across windows or
  restarts. Peek is off by default; each panel that opts in declares
  a preferred side (left / right / bottom) and a peek keybinding
  separate from its open-as-pane keybinding. The peek surface is
  *not* the codon-default placement — it is a deliberate escape
  hatch for cases where a side rail genuinely beats a pane
  (committing while staging hunks, watching the agent stream while
  editing).

- {#c-persistence} the adapter participates fully in
  [`LayoutSnapshot`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs).
  Each adapter-hosted pane round-trips through
  `capture_layout` / `apply_layout` keyed by panel type (the
  `persistent_name`) plus per-panel restore state where the panel
  already implements `SerializableItem`. Peek state is *not*
  persisted — peeks are ephemeral by contract.

- {#c-keymap-surface} every panel reachable via the adapter MUST
  expose two distinct actions, codon-namespaced:
  `codon_panes::Open<Name>` (open as a pane in the current pane
  split) and `codon_panes::Peek<Name>` (open as a transient dock
  peek). Default `codon.toml` binds the open variant under
  `cmd-k <chord>` and the peek variant under `cmd-k shift-<chord>`
  for every converted panel. Existing legacy bindings (the various
  `*_panel::ToggleFocus` actions) are rebound through the codon
  layer so a single TOML edit retargets them.

- {#c-dock-deprecation} codon's `Workspace` initialisation MUST stop
  registering the three Zed docks (left / right / bottom) as
  panel hosts in their current form. The peek surface from
  `#c-peek-mode` is the *only* dock placement codon ships. Vendored
  Zed code (commercial install paths, settings UI references to
  dock state) stays intact for the upstream diff; the surface
  difference is in codon's `Workspace` construction, not in the
  workspace crate itself.

- {#c-migration-prior-art} this REQ supersedes the deferred /
  wontdo'd per-panel attempts:
  [TASK:phase-3/agent-pane-conversion](spec:TASK:phase-3/agent-pane-conversion)
  is resolved by the adapter (the agent stops being a special case);
  [TASK:phase-4/git-panel-modal-integration](spec:TASK:phase-4/git-panel-modal-integration)
  keeps the dispatch-context / mode-tracker patches as-is (they
  remain valid for both adapter-hosted and peek-hosted placements);
  [TASK:phase-4/git-status-pane](spec:TASK:phase-4/git-status-pane)
  stays wontdo, but the *clause* it was meant to satisfy
  (`REQ:codon/git-pane#c-status`) is now re-satisfied via the
  adapter rather than via a duplicated pane.
:::

## Implementation notes

**Crate placement.** A new crate `crates/codon-panes/` houses the
adapter, the peek surface controller, and the `codon_panes::Open*`
/ `Peek*` action registrations. Per-panel converters are thin: each
is a `pub fn open_agent_pane(workspace, window, cx)` /
`pub fn peek_agent(workspace, window, cx)` pair that constructs the
panel (the panel's existing `load` constructor still works because
the adapter is transparent) and hands it to either `Workspace::split`
or the peek controller.

**Vendored Zed touchpoints.** The Panel trait itself does not need
to change — the adapter consumes it as-is. Two small additions are
expected:

1. `Workspace` gains a public helper to insert an arbitrary
   `Box<dyn ItemHandle>` into the active pane (codon's `split` flow
   already does this; this clause may or may not need new
   surface — to be confirmed during `panel-item-adapter`).
2. The peek surface reuses Zed's existing `Dock` widget for the
   visuals but is owned by codon — Zed's `Workspace::left_dock` /
   `right_dock` / `bottom_dock` fields are unused by codon at
   runtime once `#c-dock-deprecation` lands.

**Out of scope.** The status bar, the title bar, notification toasts,
and the various non-Panel chrome elements stay where they are. This
REQ is strictly about the seven `impl Panel` views.
