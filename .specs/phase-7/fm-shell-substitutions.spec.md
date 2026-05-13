---
id: TASK:phase-7/fm-shell-substitutions
type: task
status: accepted
version: 0.0.1
summary: >
  Pure-function `apply_substitutions` — expands {path} / {paths} /
  {name} / {names} / {cwd} / {parent} in command strings with proper
  shell escaping.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-shell-exec#c-shell-substitutions
---

# File-manager shell-exec substitutions

## What ships

A standalone helper:

```rust
pub fn apply_substitutions(
    template: &str,
    cursor: &Path,
    marked: &[PathBuf],
    cwd: &Path,
) -> String;
```

Replaces each placeholder with the appropriately shell-escaped
value:

- `{path}`   → cursor (shell-quoted)
- `{paths}`  → marked entries (shell-quoted, space-separated; falls
                back to `[cursor]` when marked is empty)
- `{name}`   → cursor.basename (shell-quoted)
- `{names}`  → marked basenames (shell-quoted, space-separated)
- `{cwd}`    → cwd (shell-quoted)
- `{parent}` → cwd.parent (shell-quoted)

Shell-quoting via the `shlex` crate (or a small inline quoter if
shlex isn't already in the workspace graph). The placeholders are
literal-match — `{}` braces are escaped via doubling (`{{` and
`}}`) for any user who wants literal braces in their command.

## Where it slots in

A free function in
[`crates/file-manager/src/`](spec:src:crates/file-manager/src/) —
either a new `shell.rs` module or inlined with the blocking-exec
implementation. Consumed by both
TASK:phase-7/fm-shell-blocking and TASK:phase-7/fm-shell-async.
Includes unit tests for each placeholder. ~120 LOC including
tests.
