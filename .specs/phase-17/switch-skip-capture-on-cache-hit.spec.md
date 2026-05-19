---
id: TASK:phase-17/switch-skip-capture-on-cache-hit
type: task
status: draft
version: 0.0.1
summary: >
  On intra-session window switch, skip `swap::capture` for the
  outgoing window — the runtime cache holds the live `Member`
  tree, so a fresh `LayoutSnapshot` is only useful on eviction or
  detach. Build it lazily.
owners: [carlo]
progress: done
refines:
  - REQ:codon/perf-switch#c-skip-capture-on-cache-hit
aspects: [lazy-capture, cache-hit-path, eviction-hook]
---

# Skip LayoutSnapshot capture on cache-hit window switch

## What changes

In `crates/codon-session/src/actions.rs::cycle_window` (and the
`stash_outgoing` helper used by session-switch), the current
sequence is:

```rust
let snapshot = swap::capture(workspace, window, cx);   // expensive
let runtime  = capture_runtime(workspace, cx);         // cheap (Arc bumps)
if let Some(active) = session.active_mut() {
    active.layout = Some(snapshot);                    // store fallback
}
// stash runtime in cache
```

Replace with:

```rust
let runtime = capture_runtime(workspace, cx);
// Mark the outgoing window's `layout` as STALE (snapshot lags the
// runtime cache). It will be (re-)materialised when the cache
// entry is evicted, or on session detach.
if let Some(active) = session.active_mut() {
    active.layout_stale = true;
}
```

Add a stale-snapshot materialisation hook on `WindowRuntimeCache`:

```rust
impl WindowRuntimeCache {
    /// Drop the entry for `(session_id, window_id)`. If the entry
    /// existed, materialise a fresh LayoutSnapshot from its
    /// `Member` tree and write it back to the registry.
    pub fn evict_and_persist(
        &self,
        session_id: SessionId,
        window_id: WindowId,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut App,
    );
}
```

This hook is called from:
- The cache's LRU eviction path (when a session is bumped out by
  another).
- `detach_session` (the user explicitly steps away from a
  session).
- The shutdown drain path.

The `Window::layout_stale` field is added to
`crates/codon-session/src/session.rs` next to `layout`. Default
`false`; flipped to `true` on stash, back to `false` after
materialisation.

## Why this clause

`swap::capture` walks every pane and serializes every item handle
on every switch. The runtime cache already keeps the live tree
alive — we don't need a second copy until eviction. Skipping
this saves the bulk of the synchronous outgoing-stash cost,
which the profile shows scales linearly with pane count.

## Verification

- New test `cycle_window_skips_capture_on_cache_hit` instruments
  a counter inside `swap::capture` and asserts that two
  consecutive window switches within the same session do not
  invoke `capture` for the outgoing window.
- New test `eviction_materialises_stale_snapshot` triggers an
  eviction and asserts that the registry's `Window::layout`
  field is populated with a fresh snapshot afterwards.
- Existing session/window persistence tests pass unchanged.
- `cargo clippy -p codon-session` is clean.

## Done when

- `cycle_window` and `stash_outgoing` no longer call
  `swap::capture` on the cache-hit path.
- `WindowRuntimeCache::evict_and_persist` exists and is called
  from the LRU + detach + shutdown paths.
- `spec lint` is at zero errors.
