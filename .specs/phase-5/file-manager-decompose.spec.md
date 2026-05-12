---
id: TASK:phase-5/file-manager-decompose
type: task
status: accepted
version: 0.1.0
summary: >
  Initial split: rendering extracted from the monolithic
  file_manager.rs into a sibling `view.rs` module. The full
  four-way split (state / fs_io / handlers / render) is partially
  done — file_manager.rs went from 1345 → 921 LOC, view.rs is 469
  LOC. Further extraction (handlers into a separate module) is
  worth doing only when the FM crate grows another feature; today
  the handler logic is tightly enough coupled to FileManager state
  that a free-function form would require more pub(crate) plumbing
  than the maintainability win justifies.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-module-decomposition
---

# Decompose `file_manager.rs`

## What ships

The file at
[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
is currently a 1209-line monolith holding the `FileManager` view, the
`DirEntry` data model, the read-dir / preview I/O, all the navigation
+ mutation handlers, every column-rendering helper, and the
`Render` / `Focusable` / `Item` / `EventEmitter` trait impls.

Four natural seams exist:

1. **`state.rs`** — `DirEntry`, `PendingInput`, `Mark` set, selection
   index, hidden-toggle flag. Pure data + small methods. ~150 LOC.
2. **`fs_io.rs`** — `read_dir_sync`, preview-loading, the directory-vs-file
   probe used to populate the third column. The only async/blocking
   I/O in the crate. ~120 LOC.
3. **`handlers.rs`** — `nav_*`, `create_*`, `rename_entry`,
   `delete_entry`, `toggle_hidden`, `mark_toggle`, etc. These are the
   `cx.dispatch_action`-bound entry points. ~250 LOC.
4. **`render.rs`** — column rendering (parent / current / preview),
   row decoration (mark stripe, git indicator slot, hidden dimming),
   `Render for FileManager`. ~450 LOC.

`file_manager.rs` itself shrinks to ~250 LOC: the `FileManager`
struct, `new()`, focus/event plumbing, and the `Item` / `Focusable` /
`EventEmitter` glue that wants to live next to the struct definition.

## Why this is non-trivial

- Handler functions today close over `&mut self`, `&mut Window`,
  `&mut Context<Self>`. Moving them to a sibling module means either
  (a) re-expressing them as free functions taking `&mut FileManager`,
  or (b) keeping them as inherent impl blocks split across files via
  `impl FileManager { ... }` in `handlers.rs`. Option (b) preserves
  the call sites verbatim and is the simpler refactor — pick (b)
  unless a handler needs to drop into a helper that's hard to express
  as `&mut self`.
- `render.rs` will need access to `DirEntry` (from `state.rs`) and to
  several `FileManager` fields. Either expose those fields as `pub(crate)`
  or pass them in as render-context structs. Match what
  [`vendor/zed/crates/project_panel/`](spec:src:vendor/zed/crates/project_panel)
  already does — it has the same shape and resolves it with
  `pub(super)` visibility.
- The `is_hidden` / `size` decisions from TASK:phase-5/clippy-baseline
  should land **first**. Don't decompose code you're about to delete.

## Sequencing

This task is blocked by TASK:phase-5/clippy-baseline (smaller delta,
less to rebase). Once clippy is green, decompose. The
fm-fuzzy-filter, fm-git-indicators, fm-bulk-ops, and fm-copy-paste
tasks each add 100–300 LOC; doing them on top of the monolith pushes
the file past 2000 LOC.

## File anchors

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  — the file being split.
- [`vendor/zed/crates/project_panel/src/project_panel.rs`](spec:src:vendor/zed/crates/project_panel/src/project_panel.rs)
  — Zed's analogous file-tree pane, already split across siblings;
  good template even though codon doesn't reuse its types.

## Acceptance

- No single file in `crates/file-manager/src/` exceeds 600 LOC after
  the split.
- `cargo build -p codon` and `cargo test -p file-manager` both pass.
- The split is mechanical: no behaviour change, no field renames,
  no public API surface change outside the crate.
- A subsequent `git log --stat` over the decomposition commit shows
  net-zero LOC change in `crates/file-manager/src/` (modulo the
  `mod` declarations).

Effort: medium. The mechanical move is ~1 hour; the wiring of
visibility + render-context structs is the part that takes thought.
