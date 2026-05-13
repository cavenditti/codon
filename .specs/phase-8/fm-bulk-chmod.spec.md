---
id: TASK:phase-8/fm-bulk-chmod
type: task
status: accepted
version: 0.0.1
summary: >
  `cm` opens an input bar for octal (`755`) or symbolic (`u+x`)
  modes. Applies to marked entries via `fs::Fs::set_permissions`.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-bulk-editor#c-bulk-chmod
---

# File-manager bulk chmod

## What ships

`cm` (change-mode) on a marked set: Insert-mode prompt accepting
either octal (`755`, `0755`, `0o755`) or symbolic (`u+x`, `g-w`,
`a=r`) mode strings. Enter applies to each marked entry via
`fs::Fs::set_permissions`.

Symbolic-mode parser: standalone helper (subset of chmod's
grammar — `[ugoa]*[+-=][rwx]+` clauses, comma-separated). ~50 LOC
of pure-function parsing.

Windows: prompt is shown but submission no-ops with a toast
("chmod is a unix-only operation"). Don't error — just inform.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
new `PendingInput::Chmod { mode }` + dispatch. ~150 LOC total.
