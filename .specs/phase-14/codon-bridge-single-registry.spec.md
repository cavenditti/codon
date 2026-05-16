---
id: TASK:phase-14/codon-bridge-single-registry
type: task
status: draft
version: 0.0.1
summary: >
  Collapse the two parallel registries in
  `vendor/zed/crates/workspace/src/codon_bridge.rs`
  (`register_item_panel_kind_fn` function-pointer OnceLock +
  `register_panel_restorer` HashMap of closures) into a single
  `codon_register_pane_kind` surface.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-codon-bridge-single-registry
---

# One registry in `workspace::codon_bridge`

## What changes

`vendor/zed/crates/workspace/src/codon_bridge.rs` has two
parallel registration patterns at lines ~195–270:

- `ItemPanelKindFn` stored in an `OnceLock`, registered via
  `register_item_panel_kind_fn(fn_ptr)`.
- `PanelRestorerFn` stored in a `RwLock<HashMap<String, BoxedFn>>`,
  registered via `register_panel_restorer(kind, closure)`.

Both are codon-added (not upstream Zed) and both exist to let
codon-side crates inject pane-kind metadata into the vendored
workspace's persistence + restore path. The split is historical:
the function-pointer was the first iteration; the closure
HashMap landed when codon-panes needed per-kind restoration
logic. The two never got reconciled.

## Approach

1. Read `codon_bridge.rs:1–300` end-to-end. Note every caller of
   the two registration functions and every reader of the two
   registries.
2. Design a single registry shape that covers both uses:
   ```rust
   pub struct CodonPaneKindSpec {
       pub kind: &'static str,
       pub serialize: fn(&dyn ...) -> Value,
       pub restore: Box<dyn Fn(Value, ...) -> Option<Box<dyn ItemHandle>>>,
       // ... any other per-kind hooks
   }
   pub fn codon_register_pane_kind(spec: CodonPaneKindSpec) { ... }
   pub(crate) fn codon_pane_kind_lookup(kind: &str) -> Option<&CodonPaneKindSpec> { ... }
   ```
3. Replace the two registries with one (probably
   `RwLock<HashMap<&'static str, CodonPaneKindSpec>>`).
4. Update callers in `crates/codon-panes/src/lib.rs` (and any
   other codon-side caller) to register through the single API.
5. Verify the persistence + restore path still round-trips
   FileManager panes (per the recent `b4e5955` work).

## Coordination

This TASK touches `vendor/zed/`. Per CLAUDE.md, the workflow is:

1. Commit on the `codon` branch inside `vendor/zed/` first
   (Conventional commit prefix `feat(codon-bridge): ...` or
   `refactor(codon-bridge): ...`, plus `Spec-Ref:` trailer).
2. Run `( cd vendor/zed && ./script/clippy )` and confirm clean.
3. Commit the submodule-pointer bump in the outer repo
   (`chore: update vendored zed`).

## Non-goals

- Not changing the persistence-on-disk format. `LayoutSnapshot`
  serialization stays byte-compatible so an old saved session
  still restores.
- Not making the registry generic. Single-purpose; one kind tag,
  one spec.
- Not touching non-codon upstream registries in `workspace`. Only
  the codon-added pair.

## Files touched

- `vendor/zed/crates/workspace/src/codon_bridge.rs` — collapse
  to one registry.
- `crates/codon-panes/src/lib.rs` — adapt registration callsite.
- Any other codon-side caller of the two functions (search:
  `rg -n 'register_item_panel_kind_fn|register_panel_restorer' crates/`).

## Verification

- `( cd vendor/zed && ./script/clippy )` — clean.
- `cargo build -p codon` — clean.
- Manual smoke: open codon, split into a FileManager pane plus a
  terminal pane, switch session, switch back — both panes
  restore. Quit codon, relaunch — same panes restore from disk.
- `rg -n 'register_item_panel_kind_fn|register_panel_restorer' vendor/zed/crates/workspace/src/codon_bridge.rs`
  returns zero hits. The new `codon_register_pane_kind` is the only
  registration entry point.
