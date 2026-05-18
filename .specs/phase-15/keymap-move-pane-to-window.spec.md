---
id: TASK:phase-15/keymap-move-pane-to-window
type: task
status: draft
version: 0.0.1
summary: >
  Add `codon_session::MovePaneToWindow(usize)` — move the active
  pane into an existing window by index, preserving items and
  focus. Bind `prefix shift-<N>` for N=1..9 in defaults. Mirrors
  tmux `join-pane -t :N`.
owners: [carlo]
progress: done
refines:
  - REQ:codon/keymap#c-move-pane-to-window
---

# Move pane to window N

## What changes

Codon has
[`codon_session::BreakPaneToWindow`](spec:src:crates/codon-session/src/break_pane.rs)
— promote the active pane into a fresh window — but no equivalent
for an *existing* window. tmux's `join-pane -t :N` does the latter
and is the natural fit for `prefix shift-<N>`.

Action shape mirrors `WindowGoto(usize)`:

```rust
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = codon_session)]
#[serde(deny_unknown_fields)]
pub struct MovePaneToWindow(pub usize);
```

Implementation reuses `break_pane.rs`'s snapshot surgery:

- The existing helper detaches a pane subtree from one window's
  `Member` tree. Factor that out (if not already a public helper)
  into something like `break_pane::detach_active_pane(snapshot, …) -> DetachedPane`.
- Add a sibling `break_pane::attach_to_window(snapshot, target_idx, detached)`
  that grafts the detached pane onto the target window's `Member`
  tree as a new horizontal split (consistent with how
  `codon_register_pane_kind` restores pane kinds).
- Handler in `crates/codon-session/src/actions.rs`:
  `handle_move_pane_to_window(workspace, action, …)`:
  - Resolve the active session via `SessionRegistry`.
  - Reject if `target_idx == active_window_idx` (no-op + trace).
  - Reject if `target_idx >= session.windows.len()` (silent + toast,
    same pattern as `WindowGoto` out-of-range).
  - Call `detach_active_pane` on the active window snapshot.
  - If the source window now has zero panes, close it (same single-
    pane handling as `BreakPaneToWindow`).
  - Call `attach_to_window` on the target window snapshot.
  - Persist via `serialize_workspace_now` so cross-restart restore
    sees the new layout.
  - Switch focus to the target window (via existing
    `handle_window_goto` plumbing) and to the moved pane within
    it.

Default bindings (added to `DEFAULT_KEYMAP`):

```toml
"prefix shift-1" = "codon_session::MovePaneToWindow(0)"
"prefix shift-2" = "codon_session::MovePaneToWindow(1)"
...
"prefix shift-9" = "codon_session::MovePaneToWindow(8)"
```

Update sites:

- `crates/codon-session/src/actions.rs` — declare the action,
  wire `register_for_workspace`.
- `crates/codon-session/src/break_pane.rs` — factor the detach
  helper and add `attach_to_window`.
- `crates/codon-session/src/codon_session.rs` — re-export
  `MovePaneToWindow`.
- `crates/codon-keymap/src/keymap.rs` — extend `DEFAULT_KEYMAP`
  with the nine bindings.
- `assets/config/codon.example.toml` — document the new chord
  family in the windows section.

Tests:

- `move_pane_to_window_grafts_subtree` — set up a session with
  two windows, three panes in window 0; move the active pane to
  window 1; assert window 0 now has two panes, window 1 has two
  panes, items preserved.
- `move_pane_closes_source_when_last` — single-pane source closes
  the source window.
- `move_pane_oob_index_is_noop` — `MovePaneToWindow(99)` against
  a 3-window session leaves layout unchanged and surfaces a toast.
- `move_pane_to_self_is_noop` — `MovePaneToWindow(active_idx)`
  is a no-op.

## Why this clause

The user-requested `cmd-shift-<N>` chord family has no action to
point at today; binding without implementation would log a
warning per chord at every keymap load. Implementing the action
unlocks the binding *and* gives codon the missing tmux verb.

## Done when

- `MovePaneToWindow(usize)` action exists, is registered for the
  workspace, and is bound `prefix shift-1..9` by default.
- All four tests above pass.
- Session layout persists across a restart with the moved pane in
  the target window.
- `spec lint` reports zero errors.
- `vendor/zed/script/clippy` reports no new warnings.
