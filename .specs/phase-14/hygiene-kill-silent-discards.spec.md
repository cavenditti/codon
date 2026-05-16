---
id: TASK:phase-14/hygiene-kill-silent-discards
type: task
status: draft
version: 0.0.1
summary: >
  Eliminate every silent `let _ = <fallible>` in production codon
  code. Replace with `.log_err()`, propagate with `?`, or document
  at the callsite why the error is intentionally dropped. Parameter
  shadowing (`let _ = window;` to silence warnings) is allowed and
  not in scope.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-no-silent-discards
---

# Kill silent `let _ =` discards

## What changes

The audit found 21 `let _ =` discards in production codon code. The
highest-impact cluster is on the keymap / settings load path,
where a parse failure today is silently swallowed and the user
sees no error.

## Confirmed callsites

- `apps/codon/src/zed.rs:472, 473, 474, 475, 482, 483, 485, 488, 489`
  — 9× `let _ = settings.update(...)` in the settings load loop.
  Replace with `.log_err()` chained through (the `LogErrorExt` trait
  is already imported elsewhere in the codebase) so any parse
  failure shows up in `~/.local/state/codon/logs/Codon.log`.
- `apps/codon/src/zed.rs:530, 531, 538` — 3× `let _ =` on
  task / modal operations. Audit each; most should be
  `.log_err()` since the operation is best-effort.
- `crates/codon-session/src/actions.rs:200, 783` — 2× `let _ =` on
  cache operations (window stash remove / cleanup). These are
  genuinely "best effort"; replace with a one-line comment
  documenting that and `.log_err()`.
- `crates/codon-session/src/window_rename.rs:98` — `let _ = window` is
  parameter shadowing, NOT a fallible discard. Allowed.
- `crates/codon-command-palette/src/modal.rs:238` —
  `let _ = command_label` flagged "kept for future use / aside".
  Decision: remove the parameter from the function signature
  entirely. If a future feature needs it, restore it then.
- `crates/codon-keymap/src/cheatsheet_modal.rs:967` — `let _ = forward`
  parameter shadowing. Allowed.
- `crates/file-manager/src/trash.rs`,
  `crates/file-manager/src/bulk_rename_editor.rs`,
  `crates/file-manager/src/file_manager.rs` — 5 more across these
  three files; audit per-callsite.

## Approach

1. For each callsite:
   - If parameter shadowing (`let _ = some_param;` with no `?`/await/
     fallible expression on the RHS): allowed, leave it.
   - If genuinely best-effort: chain `.log_err()` and add a one-line
     comment naming what the failure mode is.
   - If accidentally swallowed: propagate with `?` or surface via
     toast (file-manager file ops).
2. Re-grep at the end. Only parameter-shadowing hits remain.

## Non-goals

- Not refactoring `LogErrorExt`. It already exists in vendored Zed
  and is the right primitive.
- Not introducing toasts for paths that aren't user-driven (cache
  cleanup, etc.). `.log_err()` is enough for those.

## Files touched

- `apps/codon/src/zed.rs` — settings load loop (9 sites) + 3 misc.
- `crates/codon-session/src/actions.rs` — 2 sites.
- `crates/codon-command-palette/src/modal.rs` — remove the unused
  `command_label` parameter.
- `crates/file-manager/src/{trash,bulk_rename_editor,file_manager}.rs`
  — 5 sites.

## Verification

- `cargo build -p codon` — clean.
- ```
  rg -n 'let _ =' crates/codon-* crates/file-manager apps/codon/src \
    | rg -v '#\[cfg\(test\)\]|tests/'
  ```
  Returns only parameter-shadowing hits (`let _ = window;` with a bare
  identifier on the RHS).
- Manual smoke: write a syntactically-broken setting into
  `~/.config/codon/settings.json`, launch codon — an error appears
  in the log (today the failure is silent).
