---
id: TASK:phase-16/shell-output-actions
type: task
status: draft
version: 0.0.1
summary: >
  Register `vim::ShellInsertOutput` and `vim::ShellAppendOutput`
  in the vendored vim crate. Both prompt for a shell command, spawn
  `$SHELL -c <cmd>`, and insert stdout *before* (Insert) or *after*
  (Append) each selection. Bound to `!` and `alt-!`.
owners: [carlo]
progress: done
refines:
  - REQ:codon/shell-integration#c-actions-five
  - REQ:codon/shell-integration#c-prompt-ux
  - REQ:codon/shell-integration#c-multi-cursor
  - REQ:codon/shell-integration#c-insert-semantics
  - REQ:codon/shell-integration#c-append-semantics
  - REQ:codon/shell-integration#c-stderr-routing
  - REQ:codon/shell-integration#c-binding-defaults
  - REQ:codon/shell-integration#c-shell-env
  - REQ:codon/shell-integration#c-timeout-safety
aspects: [actions, prompt, multi-cursor, insert, append, stderr, bindings, shell-env, timeout]
---

# Shell insert / append output (`!` and `Alt-!`)

## What changes

Build on the shared helpers introduced in
[`phase-16/shell-pipe-action`](spec:.specs/phase-16/shell-pipe-action.spec.md).
The difference is the application semantics:

- **`ShellInsertOutput`** — for each selection, run `$SHELL -c
  <cmd>` (with **no stdin** — Helix's `!` doesn't pipe the
  selection in; it just runs the command and inserts the output
  *before* the selection). Insert stdout at the selection's start.
- **`ShellAppendOutput`** — same as above, but insert stdout at
  the selection's end.

Behavior notes from Helix:

- The command receives no stdin. Helix's `!` is "insert command
  output", not "pipe selection through command then insert". This
  is the only verb in the five that doesn't take stdin.
- One-cursor case: insert before / after the cursor's position
  (treat the cursor as a zero-width selection).
- Output trailing newline: Helix strips one trailing `\n` from the
  command's stdout before inserting. Mirror that. Codon should
  also strip a trailing `\r\n` on Windows (not currently a target,
  but cheap to do).

Extend `run_for_each_selection` from
`phase-16/shell-pipe-action` to accept a `ShellMode` enum:

```rust
enum ShellMode {
    PipeReplace,     // | : stdin=selection, replace with stdout
    PipeDiscard,     // alt-| : stdin=selection, ignore stdout
    InsertBefore,    // ! : no stdin, insert stdout before selection
    AppendAfter,     // alt-! : no stdin, insert stdout after selection
    KeepIfZero,      // $ : stdin=selection, keep selection on exit==0 (see sibling task)
}
```

The helper already exists from the pipe task; this task adds the
`InsertBefore` and `AppendAfter` arms.

Codon bindings:

```toml
[bindings.editor.normal]
"!"     = "vim::ShellInsertOutput"
"alt-!" = "vim::ShellAppendOutput"
```

## Why this clause

The insert/append pair is what most Helix users reach for first —
"give me the output of `date` here" or "append a `wc -l` of the
current buffer." It's lower-stakes than pipe (no destruction of
existing content) and easier to debug while iterating on the
shared `run_for_each_selection` plumbing.

## Verification

- Empty selection at the cursor. `!` → `date`. Date string
  appears before the cursor.
- Three multi-cursor positions. `Alt-!` → `uuidgen`. Three
  distinct UUIDs appear after each cursor.
- `!` → `does-not-exist`. Toast appears; buffer unchanged.
- `!` → `sleep 10`. After 5 s, timeout kills; toast appears.
- Undo. All inserts revert in one transact.

## Done when

- `vim::ShellInsertOutput` and `vim::ShellAppendOutput` are
  registered and bound in codon TOML.
- The shared `run_for_each_selection` handles
  `ShellMode::InsertBefore` / `AppendAfter`.
- Trailing newline stripping matches Helix.
- A unit test covers the four modes with `cat`, `echo "x"`, and a
  zero-exit / non-zero-exit command.
- `vendor/zed/script/clippy` reports no new warnings.
- `spec lint` is at zero errors.
