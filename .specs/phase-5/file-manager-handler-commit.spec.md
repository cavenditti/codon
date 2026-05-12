---
id: TASK:phase-5/file-manager-handler-commit
type: task
status: superseded
version: 0.1.0
summary: >
  Marked wontdo — verification during TASK:phase-5/clippy-baseline
  showed the commit phase already exists in `handle_insert_key`
  (lines 530–562 pre-baseline). The original scoping assumed three
  setup handlers were stubs awaiting commit work; in fact only
  their `window` params were unused because commit happens
  elsewhere. The error-surfacing + trash-delete portion of the
  original scope shipped under TASK:phase-5/clippy-baseline. The
  remaining concern — std::fs in create/rename ignoring the Fs
  trait — is now tracked under
  TASK:phase-5/file-manager-fs-trait-create-rename.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/file-manager#c-file-ops
  - REQ:codon/code-quality#c-error-visibility
aspects: [create-rename-commit-phase, fm-error-surface]
---

# Finish the create / rename commit phase

## What ships

Three handlers in
[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
are structurally incomplete:

- `create_file` (line 380)
- `create_directory` (line 386)
- `rename_entry` (line 392)

Each one sets `self.pending_input = Some(PendingInput { kind, … })`
and re-renders to show the inline prompt — but no handler ever reads
`pending_input` to commit the user's typed value. The corresponding
keymap entries (`a`, `A`, `r` in
[`assets/config/keymap.example.toml`](spec:src:assets/config/keymap.example.toml))
fire, the prompt appears, the user types and hits Enter, and… nothing
happens. The clippy warning that `window` is unused on those three
handlers is a true positive: the commit phase that would consume it
was never wired.

This task adds:

1. **A `PendingInputCommit` action** dispatched on Enter inside the
   prompt, registered in codon-keymap's default TOML for the
   `file_manager.pending` context.
2. **The commit branches** — `read` the staged `kind` + text, call
   `std::fs::File::create` / `std::fs::create_dir` / `std::fs::rename`
   via the injected `Arc<dyn fs::Fs>` (NOT direct stdfs — see
   REQ:codon/code-quality#c-fs-trait-purity), then trigger a
   read_dir_sync refresh of the affected column.
3. **A cancel path** — Esc clears `pending_input` without committing.
4. **Error surfacing** — failures route to a one-line toast / banner,
   not `.log_err()`. The user must see "couldn't create — Permission
   denied" rather than nothing. Use the existing
   `workspace::Workspace::show_error` helper if it covers the case;
   otherwise add an inline status row on the file-manager pane.

## File anchors

- [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
  — handlers at lines 380/386/392, plus wherever `pending_input` is
  rendered today.
- [`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
  — register the new `file_manager::PendingInputCommit` action.

## Acceptance

- Press `a` in a file manager pane, type `foo.txt`, press Enter →
  `foo.txt` exists on disk and the pane re-reads the directory.
- Press `A`, type `bar/`, press Enter → `bar/` directory exists.
- Press `r` on an entry, type a new name, press Enter → the entry
  is renamed.
- Press Esc inside any of the three prompts → no FS change, prompt
  closes.
- Triggering a failure (e.g., rename onto an existing read-only
  path) shows a visible error message to the user. The error does
  not silently `log_err`.
- The three `window` clippy warnings at the handler sites are gone
  (they now use the param) — see TASK:phase-5/clippy-baseline.

## Out of scope

- Bulk ops (covered by TASK:phase-5/fm-bulk-ops).
- Copy / paste / yank (covered by TASK:phase-5/fm-copy-paste).
- Trash-vs-permanent deletion semantics — `d` already exists,
  this task does not touch it.

Effort: small-to-medium. ~150 LOC plus the error-surface plumbing,
which may be the biggest unknown depending on whether
`Workspace::show_error` is wired to the file-manager pane today.
