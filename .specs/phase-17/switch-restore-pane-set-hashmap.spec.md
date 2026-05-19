---
id: TASK:phase-17/switch-restore-pane-set-hashmap
type: task
status: draft
version: 0.0.1
summary: >
  Replace the O(N×M) pane-set merge inside
  `Workspace::restore_center_root` (vendored Zed) with an O(N+M)
  `HashSet`-based pass. Pure complexity fix; behaviour unchanged.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/perf-switch#c-restore-pane-set-hashmap
aspects: [vendored-zed, complexity-fix, restore-path]
---

# Pane-set HashSet in restore_center_root

## What changes

In
[`vendor/zed/crates/workspace/src/workspace.rs`](spec:src:vendor/zed/crates/workspace/src/workspace.rs)
at `fn restore_center_root`, the current loop is:

```rust
for pane in &new_panes {
    if !self.panes.iter().any(|p| p.entity_id() == pane.entity_id()) {
        self.panes.push(pane.clone());
    }
}
```

This is O(N×M) for N=new_panes, M=workspace.panes. With deeply
nested splits on both source and target windows the constant is
small but the asymptotic is wrong.

Replace with:

```rust
let mut existing_ids: HashSet<EntityId> =
    self.panes.iter().map(|p| p.entity_id()).collect();
for pane in &new_panes {
    if existing_ids.insert(pane.entity_id()) {
        self.panes.push(pane.clone());
    }
}
```

Single O(M) build of the existing set, single O(N) iteration of
the new vec. The `HashSet::insert` return value (true if newly
inserted) doubles as the "should I push" check.

## Why this clause

Pure cleanup; the cost is small per-switch but visible in the
profile during the restore burst. Doing the right thing here also
sets up a cleaner code path for future multi-pane-restore work.

## Verification

- Existing workspace tests pass unchanged.
- A microbench (added to `vendor/zed/crates/workspace/benches/`
  if Zed's bench infrastructure supports it, otherwise as a
  `#[test] #[ignore]` timing test) asserts the new
  implementation outperforms the old for N=M=20.
- `vendor/zed/script/clippy` reports no new warnings on the
  workspace crate.

## Done when

- `restore_center_root` uses the HashSet path.
- Submodule pointer in the outer repo bumps forward.
- `spec lint` is at zero errors.
