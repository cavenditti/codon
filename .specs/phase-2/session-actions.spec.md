---
id: TASK:phase-2/session-actions
type: task
status: accepted
version: 0.1.0
summary: >
  Post-migration follow-ups on session/window actions — auto-create
  session on WindowNew and in-memory pane stash for window switching.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-create
  - REQ:codon/windows#c-swap-on-switch
aspects: [session-auto-create, in-memory-pane-stash]
---

# Session-action follow-ups

Two bug-fix follow-ups landed after the original Phase 2 tasks were
marked done.

## Auto-create session on WindowNew

`WindowNew` used to no-op with `no active session, ignoring WindowNew`
on a fresh launch. A window without a session is a tmux contradiction,
not a real ambiguity. Extracted an `ensure_active_session` helper that
creates a default cwd-anchored session if none is loaded, then proceeds.
See [codon_session::actions](spec:src:crates/codon-session/src/actions.rs).

## In-memory pane stash for window switching

The original implementation captured a `LayoutSnapshot` per window and
restored it via the workspace deserialize path. That path drops items
that don't implement `SerializableItem` (file manager) and fails for
items whose 200 ms persistence debounce hadn't fired yet (just-opened
editors), so switching back to a window dropped non-terminal panes.

Replaced with a `WindowRuntimeCache` keyed by `(SessionId, WindowId)`
that holds a cloned `Member` tree + active pane handle. Cloning Entity
refs is cheap and keeps panes (and their workspace subscriptions)
alive across switches. New `workspace::restore_center_root` re-attaches
the stashed tree without re-subscribing. The persisted snapshot is
still captured on each switch for cross-restart restoration.
