---
id: TASK:phase-12/fm-pane-restore
type: task
status: accepted
version: 0.0.1
summary: >
  FileManager implements SerializableItem so file-manager center
  panes round-trip through capture_layout / apply_layout.
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-fm-restore
---

# File-manager pane restore

## Problem

`codon_bridge::capture_layout` (in
[`vendor/zed/crates/workspace/src/codon_bridge.rs`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs))
walks the workspace center group and emits an `ItemSnapshot` per pane
item — *only* for items registered with the
`SerializableItemRegistry`. `FileManager` (in
[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs))
implemented `Item` but not `SerializableItem`, so file-manager panes
silently dropped out of every captured layout. Sessions restored their
terminals but came back missing the file manager.

## Approach

1. Add a tiny per-kind SQLite domain `FileManagerDb` in
   `crates/file-manager/src/persistence.rs`, mirroring
   `ImageViewerDb` (table `file_managers(workspace_id, item_id,
   current_dir)`). The only state that needs to round-trip is
   `current_dir` — the rest is either reconstructable from the FS
   (entries, git status, preview) or comes from global prefs (sort,
   hidden, show_gitignored, preview_fraction).
2. Implement `SerializableItem` for `FileManager` in
   `crates/file-manager/src/file_manager.rs`:
   - `serialized_item_kind = "FileManager"`,
   - `serialize` writes `current_dir` keyed on (workspace_id, item_id),
   - `deserialize` reads the path back, falls back to the project's
     first worktree root if the stored path no longer exists,
   - `should_serialize` returns `true` on `PathChanged`,
   - `cleanup` calls `workspace::delete_unloaded_items` on the
     `file_managers` table.
3. Call `workspace::register_serializable_item::<FileManager>` from
   `file_manager::init`.

`codon_bridge::capture_layout` and `apply_layout` need no changes:
once `FileManager` is in the registry, capture picks it up and
`SerializedPaneGroup::deserialize` finds its factory by kind string.

## Files touched

- Edit: `crates/file-manager/Cargo.toml` — add `db.workspace = true`.
- New: `crates/file-manager/src/persistence.rs` — `FileManagerDb`
  domain + migrations + `save_current_dir` / `get_current_dir`.
- Edit: `crates/file-manager/src/lib.rs` — declare the new module.
- Edit: `crates/file-manager/src/file_manager.rs` — `SerializableItem`
  impl + `register_serializable_item` call in `init`.

## Out of scope

Restoring per-row state (selected entry, marked set, scroll position,
back/forward history). Cheap to add later by extending the persistence
schema; not load-bearing for "session restore brings the FM back."
