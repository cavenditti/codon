---
id: TASK:phase-5/file-manager-fs-trait-create-rename
type: task
status: accepted
version: 0.0.1
summary: >
  Route file-manager's create-file / create-directory / rename
  operations through `Arc<dyn fs::Fs>` instead of `std::fs::*`,
  matching the trait-purity rule. Delete already uses `fs.trash`
  (shipped under TASK:phase-5/clippy-baseline). The three create
  /rename paths are the last std::fs callers in the crate.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-fs-trait-purity
---

# Route create / rename through `Arc<dyn fs::Fs>`

## What ships

`FileManager` already holds `fs: Arc<dyn fs::Fs>` and now uses it
for `delete_entry` via `fs.trash(...)`. Three other FS calls still
go through `std::fs::*`:

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  — `handle_insert_key` Enter branches for `PendingInput::CreateFile`,
  `PendingInput::CreateDirectory`, and `PendingInput::Rename`.

Replace each with the Fs trait's async equivalent:

- `std::fs::write(&path, "")` → `fs.create_file(&path,
  fs::CreateOptions::default()).await` (or whichever is the
  canonical empty-file creation on the trait).
- `std::fs::create_dir_all(&path)` →
  `fs.create_dir(&path).await` (Fs's `create_dir` is recursive by
  default; confirm before swapping).
- `std::fs::rename(&original, &new_path)` →
  `fs.rename(&original, &new_path, fs::RenameOptions::default()).await`.

Wrapping pattern follows the one already in place for `delete_entry`:
spawn into an async block, collect failures, hop back via
`this.update_in`, and use `surface_error` per failure plus a
post-op `reload_entries`.

## File anchors

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  — the three call sites are inside the Enter branch of
  `handle_insert_key`.
- [`vendor/zed/crates/fs/src/fs.rs`](spec:src:vendor/zed/crates/fs/src/fs.rs)
  — the Fs trait surface; `create_file`, `create_dir`, `rename`,
  `trash` are all here.

## Acceptance

- Zero `std::fs::write`, `std::fs::create_dir_all`, and
  `std::fs::rename` calls remain in `crates/file-manager/src/*`.
- The existing `surface_error` plumbing surfaces failures from the
  new trait calls — same UX as today's trash-delete path.
- `cargo test -p file-manager` still passes (no new tests
  introduced by this task — coverage is the job of
  TASK:phase-5/file-manager-tests).
- Manual: press `a`, type `x`, Enter → file `x` appears. Press `r`,
  type `y`, Enter → rename to `y`. Force a failure (read-only dir)
  → visible red error row.

## Out of scope

- `read_dir_sync` and `update_preview_sync` still use sync stdlib
  I/O. Those run on the main thread under the assumption that
  directory reads are fast; routing them through the async Fs
  trait would require restructuring the pane's load path. Track
  separately if profiling says it matters.

Effort: small. ~40 LOC swap; the spawning structure already
exists in `delete_entry` as a template.
