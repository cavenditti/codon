---
id: TASK:phase-17/switch-restore-skip-notify
type: task
status: draft
version: 0.0.1
summary: >
  Elide the per-pane `notify()` loop at the tail of
  `Workspace::restore_center_root` for panes that were already in
  `workspace.panes` before the restore — only newly-attached panes
  need their cached render state invalidated.
owners: [carlo]
progress: done
refines:
  - REQ:codon/perf-switch#c-restore-skip-notify
aspects: [vendored-zed, notify-elision, gpui-render-cache]
---

# Elide pane notify on cache restore

## What changes

The tail of `restore_center_root` reads:

```rust
for pane in &new_panes {
    pane.update(cx, |_, cx| cx.notify());
}
cx.notify();
```

This unconditionally invalidates the GPUI render cache for every
pane in the restored tree. For panes that **stayed alive** in
`workspace.panes` through the detach/attach cycle (the entire
point of the codon runtime cache), the notify is wasted work —
and worse, it cancels exactly the cache-warmth that justifies
keeping the panes alive.

Replace with:

```rust
let prev_ids: HashSet<EntityId> = self.panes
    .iter()
    .map(|p| p.entity_id())
    .collect();  // before the new-pane merge (move the existing_ids
                 //   from the previous task's HashSet up and reuse it)
codon_collect_panes(&new_root, &mut new_panes);
// merge new panes ... (per phase-17/switch-restore-pane-set-hashmap)
for pane in &new_panes {
    if !prev_ids.contains(&pane.entity_id()) {
        pane.update(cx, |_, cx| cx.notify());
    }
}
cx.notify(); // workspace-level notify always fires
```

The outer `cx.notify()` on `Workspace` still fires so GPUI knows
the workspace itself changed; only the per-pane invalidation is
scoped to newly-attached panes.

## Why this clause

Profiling the cache-hit restore shows ~30% of restore cost is in
the per-pane notify burst — each `pane.notify()` triggers GPUI's
render invalidation, which for editor panes drops the cached
shaped-line layouts and forces a full layout re-run on the next
frame. That's the cost the runtime cache exists to avoid.

## Risk note

If a future GPUI version reaches a state where reassigning
`workspace.center` doesn't trigger pane re-render at all, the
elision becomes a correctness bug (stale content). The task
includes a regression test that triggers a restore and asserts
the first frame after restore renders fresh pane content.

## Verification

- New regression test
  `restore_center_root_renders_fresh_content_after_cache_hit`
  paints the workspace, switches windows, switches back, asserts
  the active pane's first post-restore frame reflects whatever
  the pane currently models (not a stale snapshot).
- New test
  `restore_center_root_skips_notify_for_retained_panes`
  instruments a notify counter on a pane that survives the
  detach/attach cycle and asserts it is not invalidated.
- New test `restore_center_root_notifies_truly_new_panes`
  asserts that a pane that was not in `workspace.panes` before
  the call IS notified.

## Done when

- `restore_center_root` elides notify for retained panes.
- All three regression tests pass.
- Submodule pointer bumps forward.
- `spec lint` is at zero errors.
