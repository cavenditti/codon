---
id: REQ:codon/perf-switch
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Cut codon's window-switch and session-switch synchronous cost to
  ≤ 8 ms p95 by eliminating redundant `LayoutSnapshot` captures on
  cache-hit paths, deferring `PersistedRegistry` writes off the
  switch path, fixing O(N×M) pane-set rebuilding in
  `restore_center_root`, and elision of unconditional pane re-render
  notifications.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-17]
---

# Window- and session-switch performance

## Motivation

Codon's switch path runs four expensive synchronous steps **every
switch**, even on the common case where the in-memory
`WindowRuntimeCache` will hit:

1. **Full `LayoutSnapshot` capture** of the outgoing window — walks
   the entire pane tree, reads each `Pane`'s items, and produces a
   serializable snapshot via
   [`capture_layout`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs).
   On a multi-split window with many tabs this is hundreds of
   reads + allocations.
2. **`PersistedRegistry` upsert** — `registry.upsert(session)`
   rewrites the in-memory registry. Cheap on its own.
3. **`persist_async`** — spawns a background task that
   serializes the entire `PersistedRegistry` (every session, every
   window, every layout) to JSON and writes the KVP key. The
   serialization itself happens on a background thread but the
   queue depth scales with switch rate; under rapid `prefix Tab`
   mashing the task queue grows linearly.
4. **`restore_center_root`** does O(N×M) pane-set merge against
   `workspace.panes` and unconditionally calls `pane.notify()` on
   every restored pane, forcing GPUI to invalidate every pane's
   render cache.

The overview-open path (`SessionOverview` / `WindowOverview`
actions) does an *additional* `swap::capture` purely to populate
the modal — even if the user dismisses the modal immediately.

None of (1, 3, 4-notify) are necessary on the cache-hit path. The
runtime cache already holds the live `Member` tree; capturing a
snapshot whose only consumer is the on-disk fallback is wasted
work until eviction. Persisting the registry whose only change is
`active_window` is wasted JSON.

## Requirement

:::{requirement id="perf-switch" level="MUST"}
The system MUST:

- {#c-skip-capture-on-cache-hit} on intra-session window switch,
  skip building a `LayoutSnapshot` for the outgoing window when
  the runtime cache will hold its live `Member` tree. The
  snapshot MUST be (re-)built only when the cache entry is
  evicted, the session is detached, or the application is
  shutting down. The fallback path (no runtime cache) is
  unchanged.
- {#c-defer-persist} defer `persist_async` calls triggered by
  intra-session window switches. The registry MUST be persisted:
  (i) on session attach / detach, (ii) on session creation /
  deletion / rename, (iii) on a debounced idle timer (≥ 2 s
  after the last switch), and (iv) on shutdown. Rapid
  consecutive window switches MUST coalesce to at most one
  background persist task.
- {#c-restore-pane-set-hashmap} `restore_center_root` MUST
  perform the new-panes merge in O(N+M): build a
  `HashSet<EntityId>` of currently-known pane ids in a single
  O(M) pass, then iterate the incoming pane vec in O(N), pushing
  only previously-unseen panes. The behaviour is unchanged; only
  the complexity drops.
- {#c-restore-skip-notify} `restore_center_root` MUST elide the
  per-pane `notify()` for panes that were already present in
  `workspace.panes` before the restore. The notify is necessary
  only for newly-attached panes whose cached render state is
  stale; panes that lived through the detach/attach cycle do not
  need to be invalidated, and forcing them defeats the cache.
- {#c-overview-defer-capture} `SessionOverview` and
  `WindowOverview` action handlers MUST NOT call
  `swap::capture(workspace, ...)` on the modal-open path. The
  modal MUST source its layout summary from the last cached
  snapshot held by the active session's `Window::layout` field,
  capturing fresh state lazily (on-demand) when the modal
  actually renders a tile that needs it.
- {#c-switch-budget-harness} expose the same render-trace
  framework introduced for the FM render pipeline
  ([`TASK:phase-17/fm-render-frame-budget`](spec:TASK:phase-17/fm-render-frame-budget))
  with an additional `SwitchTiming` event capturing per-switch
  durations for capture / restore / persist phases. The
  acceptance gate: switch from a 4-pane window to a 4-pane
  window in another session MUST measure ≤ 8 ms p95 (no first-time
  cache miss), ≤ 16 ms p95 on first switch with cache miss
  (incurs the persisted-snapshot apply path).
:::

## Trade-offs

- **Crash-safety regression on intra-session switches.** Deferring
  registry persistence means a `kill -9` between switches loses
  the *latest active-window pointer* (the session list itself is
  unaffected). This is an accepted trade — codon prioritises
  switch responsiveness over millisecond-grained crash recovery,
  and the idle-debounced flush still bounds the loss to
  ~2 seconds of activity.
- **Snapshot freshness on eviction.** Deferring capture until
  eviction means the on-disk fallback always lags the live tree
  by one switch. Acceptable: the fallback is only consulted on
  cold start, where it is already at least one app-lifetime
  stale.
- **Notify-elision correctness.** Skipping `notify` on retained
  panes assumes GPUI continues to render the pane when the
  workspace's `center` field is reassigned. If a future GPUI
  version reaches a state where reassignment doesn't trigger
  re-render, the elision becomes incorrect. The
  `c-restore-skip-notify` task includes a regression test for
  this case.
