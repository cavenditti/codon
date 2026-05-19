---
id: TASK:phase-16/shell-keep-pipe
type: task
status: draft
version: 0.0.1
summary: >
  Register `vim::ShellKeepPipe` in the vendored vim crate.
  Prompts for a shell command, pipes each selection through
  `$SHELL -c <cmd>`, and keeps only those selections whose exit
  code was 0. Buffer text is unchanged. Bound to `$`.
owners: [carlo]
progress: done
refines:
  - REQ:codon/shell-integration#c-actions-five
  - REQ:codon/shell-integration#c-prompt-ux
  - REQ:codon/shell-integration#c-multi-cursor
  - REQ:codon/shell-integration#c-keep-pipe-semantics
  - REQ:codon/shell-integration#c-binding-defaults
  - REQ:codon/shell-integration#c-shell-env
  - REQ:codon/shell-integration#c-timeout-safety
aspects: [action, prompt, multi-cursor, predicate, bindings, shell-env, timeout]
---

# Shell keep-pipe (`$`)

## What changes

`vim::ShellKeepPipe` is the most distinctive of the five verbs:
it doesn't edit text. It uses the shell command as a *predicate*.
For each selection, spawn `$SHELL -c <cmd>` with the selection on
stdin; if exit code is 0, keep the selection in the active set;
otherwise drop it.

Use cases:

- Select all lines, `$` → `grep -q TODO`. Keeps only lines
  containing "TODO".
- Select all import statements, `$` → `python3 -c "import sys;
  exec(sys.stdin.read())"`. Keeps only imports that succeed.
- Select all changed files (multi-cursor over file paths), `$` →
  `test -s`. Keeps only non-empty files.

Implementation extends the shared `run_for_each_selection` helper
introduced in `phase-16/shell-pipe-action` with the
`ShellMode::KeepIfZero` arm:

```rust
ShellMode::KeepIfZero => {
    let kept: Vec<Range<Anchor>> = results
        .into_iter()
        .zip(selections.iter())
        .filter_map(|((exit, _stdout, _stderr), sel)| {
            (exit == 0).then(|| sel.clone())
        })
        .collect();
    if kept.is_empty() {
        // Empty match set: keep the primary selection so the user
        // doesn't lose all cursors silently. Toast informs.
        editor.toast("$: no selections matched; primary preserved", cx);
        editor.change_selections(cx, |s| s.select_anchor_ranges([selections.primary.clone()]));
    } else {
        editor.change_selections(cx, |s| s.select_anchor_ranges(kept));
    }
}
```

Codon binding:

```toml
[bindings.editor.normal]
"$" = "vim::ShellKeepPipe"
```

Edge cases:

- **All selections drop.** Helix keeps no selections (no cursors).
  Codon prefers to keep the primary cursor + toast (less
  foot-gun-prone for terminal-first users who may not realize
  they've lost all cursors). Document the divergence in the
  binding comment.
- **Timeout.** Treat killed processes as non-zero → drop.
- **Stderr.** Ignore; the predicate's "answer" is the exit code.
  No toast for non-zero exits (that's the normal path here).

## Why this clause

`$` is the verb that turns multi-cursor selections into a
*query*: pick a regex-like predicate that's hard to express in
regex (run an interpreter, check exit), and let the shell answer
yes/no per selection. Pairs naturally with `s` (select-regex) and
`K`/`Alt-K` (keep/remove by regex) for compound refinements.

## Verification

- Select all lines in a file. `$` → `grep -q TODO`. Cursors
  narrow to lines containing "TODO".
- Select all words. `$` → `grep -q '^[A-Z]'`. Cursors narrow to
  Capitalized words.
- Pass a command that exits 0 for all selections. All cursors
  preserved.
- Pass a command that exits non-zero for all selections. Toast
  appears; primary cursor preserved.
- Pass a slow command. Timeout fires; cursors drop or are
  preserved per the timeout rule above.

## Done when

- `vim::ShellKeepPipe` is registered and bound in codon TOML.
- Multi-cursor predicate evaluation is parallel + timeout-safe.
- All-drop case preserves primary cursor with a toast.
- A unit test covers a 3-cursor set with mixed exit codes.
- `vendor/zed/script/clippy` reports no new warnings.
- `spec lint` is at zero errors.
