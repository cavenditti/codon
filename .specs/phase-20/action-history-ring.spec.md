---
id: TASK:phase-20/action-history-ring
type: task
status: draft
version: 0.0.1
summary: >
  Track the last N (default 10) non-motion actions fired by the user
  in a global ring. Bind `.` (Normal mode in every pane) to re-fire
  the most recent entry against the currently-focused pane, and bind
  `prefix ;` to a picker over the ring with chord hints rendered per
  entry. Each action declares `is_motion: bool` (default false); only
  non-motion entries enter the ring.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/discoverability#c-action-history-ring
blocked_by:
  - TASK:phase-20/fm-hidden-rebind
---

# Action history ring

## Plan

### Storage

New crate `codon-history` (or a submodule of `codon-keymap` if the
surface stays small):

```rust
pub struct HistoryEntry {
    pub action_name: String,
    pub payload: Option<serde_json::Value>,
    pub fired_at: std::time::Instant,
    pub origin_pane_kind: PaneKind,   // diagnostic only
}

pub struct History {
    entries: VecDeque<HistoryEntry>,
    cap: usize,
}

impl gpui::Global for History {}
```

Cap default = 10, read from
`~/.config/codon/codon.toml` `[history] capacity = N` (new top-level
table; loader extension trivial).

### Dispatcher hook

Today the codon-keymap loader binds chords via
[`gpui::App::bind_keys`](spec:src:vendor/zed/crates/gpui/src/key_dispatch.rs).
Action dispatch flows through GPUI's matcher → focused element's
`on_action` handlers. To intercept *every* fired codon-side action
without touching each `on_action`, the cleanest path is a focus
subscriber registered alongside the existing one in
[`codon-mode::install_pane_mode_dispatcher`](spec:src:crates/codon-mode/src/dispatcher.rs)
that observes `cx.observe_actions` (if available) or wraps the
`build_action` path in `crates/codon-keymap/src/keymap.rs` to emit
a "fired" event into the history.

If GPUI doesn't expose a global "action fired" hook, the fallback
is an explicit `codon_history::record(...)` call sprinkled at each
codon-side `actions!` dispatch site — tedious but tractable since
codon-side actions are a closed set.

Pick the cleanest mechanism at implementation time; document the
choice in the merge commit.

### Motion classification

Each codon-side action declares `is_motion: bool`. Examples:

- Motion (excluded): `workspace::ActivatePaneLeft/Right/Up/Down`,
  `workspace::SwapPane*`, `codon_session::ResizePane*`,
  `pane::ActivateNextItem/PreviousItem`, `vim::*` motion verbs
  (`Down`, `Up`, `WrappingLeft`, …), `vim::HelixCollapseSelection`,
  `editor::SelectLargerSyntaxNode`, etc.
- Non-motion (included): `codon_session::SplitRight`,
  `codon_session::Close`, `codon_agent::AgentExplain`,
  `git::StageFile`, `editor::Paste`, `vim::HelixSubstitute`, etc.

Provide a curated default classification table in
`codon-history/src/classification.rs`; the table is the source of
truth (cheaper than tagging every action). The table is keyed on
action-name strings and is overridable by
`~/.config/codon/codon.toml` `[history.motion]` (a map of
`"action_name" = bool`) for power-user tweaks.

### Repeat action

`codon_keymap::RepeatLast` — re-fires `entries.back()` against the
currently focused pane via `cx.dispatch_action(...)`. Importantly:
*the action is re-dispatched at the focused element, not at the
origin pane*. This is the lever that makes `.` effectively context-
aware ("repeat the last verb here").

Bind under the new `[bindings.global.normal]` predicate (introduced
by [TASK:phase-20/space-leader-pickers](spec:TASK:phase-20/space-leader-pickers)):

```toml
"." = "codon_keymap::RepeatLast"
```

### History picker

`codon_keymap::HistoryPicker` — opens a codon-pickers modal
([`crates/codon-pickers/`](spec:src:crates/codon-pickers/)) over
the history ring. Each row shows:

```
<action display name>     <chord if bound>     <fired N s ago>
```

Confirming a row re-fires the action against the focused pane.
Bind:

```toml
"prefix ;" = "codon_keymap::HistoryPicker"
```

### Payload capture

Actions with a payload (`WindowGoto(usize)`, `SelectRegister("a")`)
store the resolved JSON payload at fire time so re-fire is
deterministic. Actions whose payload references a transient
selection (e.g. `codon_agent::AgentExplain` seeded from a
selection) MUST capture the selection at fire time too — otherwise
re-fire would silently use the new focused pane's current selection,
which surprises users. Implementation: codon-side actions that
consume a selection serialise the selection into their payload
before dispatch (most already do via `codon-pane-bridge`'s
`SelectionSource`).

### Persistence

The ring is in-memory only across process restarts. Persisting it
adds invariant complexity (replay an action that referenced a
torn-down pane?) without obvious user value. Document the decision
in the merge commit; revisit if a user signal requests it.

## Acceptance

- `cargo test -p codon-history` (or wherever the ring lives)
  covers: motion-action exclusion, payload capture, cap eviction,
  repeat dispatching to focused pane.
- Manual smoke: fire `codon_session::Close` in a terminal pane,
  switch to a file-manager pane, press `.` — the file-manager item
  closes (not the terminal that originally received the action).
- `prefix ;` opens a picker listing the last 10 non-motion
  actions; arrow-keys + enter re-fires the highlighted one.
- Motions (`h/j/k/l`, pane focus chords, helix motions) never
  appear in the ring.
- `spec lint` clean.

## Files touched

- New `crates/codon-history/` (or a module under `codon-keymap`).
- `crates/codon-keymap/src/keymap.rs` — bind `.` and `prefix ;`
  under `[bindings.global.normal]`.
- `crates/codon-pickers/src/` — new picker variant for the ring.
- `apps/codon/src/main.rs` — wire the history `Global` and the
  classification table at init time.
- Tests across the new crate.
