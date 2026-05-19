---
id: TASK:phase-17/switch-defer-persist
type: task
status: draft
version: 0.0.1
summary: >
  Defer `PersistedRegistry` JSON writes off the synchronous switch
  path. Persist on session lifecycle events (attach / detach /
  create / delete / rename), on a ≥ 2 s idle timer after the last
  switch, and on shutdown — but never on every keystroke that
  cycles a window.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/perf-switch#c-defer-persist
aspects: [debounce, idle-flush, lifecycle-hooks, persist]
---

# Debounced registry persistence

## What changes

`crates/codon-session/src/actions.rs::persist_async` is called
from `cycle_window`, `stash_outgoing`, and several other action
handlers. Today every call spawns a fresh background task that
serializes the entire `PersistedRegistry`.

Introduce a `PersistScheduler` (small struct held by
`SessionRegistry::global`):

```rust
pub struct PersistScheduler {
    dirty: AtomicBool,
    pending_timer: Mutex<Option<Task<()>>>,
}

impl PersistScheduler {
    /// Mark the registry dirty and schedule a debounced flush
    /// (2 s idle). If a flush task is already pending, do nothing.
    pub fn mark_dirty(&self, cx: &mut App);

    /// Persist now, synchronously enqueue a background task that
    /// runs immediately. Use for lifecycle events that must hit
    /// disk before returning (attach, detach, shutdown).
    pub fn flush_now(&self, cx: &mut App) -> Task<()>;
}
```

Replace the existing `persist_async(cx)` call sites:

| Call site | Old | New |
|---|---|---|
| `cycle_window` | `persist_async(cx)` | `scheduler.mark_dirty(cx)` |
| `stash_outgoing` | `persist_async(cx)` | `scheduler.mark_dirty(cx)` |
| `attach_session` | `persist_async(cx)` | `scheduler.flush_now(cx).detach()` |
| `detach_session` | `persist_async(cx)` | `scheduler.flush_now(cx).detach()` |
| `SessionNew` / `SessionDelete` / `SessionRename` | `persist_async(cx)` | `scheduler.flush_now(cx).detach()` |
| `App::on_app_quit` hook | — | `scheduler.flush_now(cx).detach()` |

The 2 s debounce coalesces rapid switches into one persist task.
Lifecycle events get an immediate flush so the on-disk view is
consistent at the boundaries that matter for crash-recovery.

## Why this clause

Today every `prefix Tab` queues a JSON serialization of the full
registry on the background executor. The work itself is on a
worker thread, but the queue grows with switch rate and the
JSON ser cost is proportional to total registry size — every
session, every window, every layout. Coalescing to a debounced
flush bounds the cost to ≤ one JSON serialization per 2 s
regardless of switch rate.

## Verification

- New test `persist_scheduler_coalesces_rapid_switches`
  simulates 10 `cycle_window` calls in 50 ms, advances time by
  1 s (no flush), then 1.1 s (one flush fires), asserts exactly
  one background task was spawned.
- New test `persist_scheduler_flush_now_on_attach` asserts
  attach/detach/create/delete/rename produce a synchronous
  background flush.
- Existing persistence tests pass unchanged.
- `cargo clippy -p codon-session` is clean.

## Done when

- `PersistScheduler` exists; all switch-path call sites use
  `mark_dirty`.
- Lifecycle events call `flush_now`.
- `spec lint` is at zero errors.
