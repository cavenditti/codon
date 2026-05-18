---
id: TASK:phase-16/shell-palette-verbs
type: task
status: draft
version: 0.0.1
summary: >
  Register five palette verbs (`sh`, `pipe`, `insert-output`,
  `append-output`, `keep-pipe`) in `codon-command-palette` so the
  shell-integration surface is reachable from the `:` palette in
  addition to the keyboard shortcuts. The `sh` verb is the
  standalone "run a command, capture output to a transient buffer"
  form Helix exposes — no selection involvement.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/shell-integration#c-palette-verbs
---

# Palette path for shell verbs

## What changes

Codon's palette has a registered-completer mechanism in
[`crates/codon-command-palette/src/completer.rs`](spec:src:crates/codon-command-palette/src/completer.rs)
for typed-argument sub-pickers (e.g., `open <path>`,
`theme <name>`). Shell verbs use a slightly different shape: the
argument is *free-form text*, not fuzzy-matched against a
candidate list. Extend the completer surface (or add a sibling
trait `FreeFormCompleter`) for this.

Five palette verbs:

```text
:sh             <cmd>      Run cmd, capture stdout+stderr in a transient buffer pane (no selection involvement).
:pipe           <cmd>      Same as keyboard `|`.
:insert-output  <cmd>      Same as keyboard `!`.
:append-output  <cmd>      Same as keyboard `alt-!`.
:keep-pipe      <cmd>      Same as keyboard `$`.
```

`:sh` is the only verb without a keyboard counterpart. It runs a
command in the project's first worktree, captures stdout + stderr,
and pipes the combined output into a *new transient buffer pane*
(or a new editor item — pick whichever codon-session's
`SplitTerminalRight`-style flow already supports). Title:
`sh: <cmd>`. The buffer is read-only; closing it discards the
output.

Implementation:

- Add `crates/codon-command-palette/src/shell_verbs.rs` (or extend
  `completer.rs` with a `FreeFormCompleter` variant; pick whichever
  keeps the file count low per codon conventions).
- Register five verbs:
  - `sh` — dispatches `codon_command_palette::RunShell(cmd)` (new
    action, takes a String payload).
  - `pipe`, `insert-output`, `append-output`, `keep-pipe` —
    dispatch the corresponding `vim::Shell*` actions, but with
    the command already filled in (skip the prompt step). This
    needs a payload variant of the vim actions: either a sibling
    action `vim::ShellPipeSelectionWith(cmd: String)` for each, or
    a generic `vim::ShellRun(mode: ShellMode, cmd: String)`. The
    generic form keeps the surface smaller.

The generic action approach:

```rust
// in vendor/zed/crates/vim/src/shell.rs
#[derive(Clone, Debug, PartialEq, Default, Deserialize, JsonSchema, Action)]
#[action(namespace = vim)]
pub struct ShellRun {
    pub mode: ShellMode,    // enum: PipeReplace, PipeDiscard, InsertBefore, AppendAfter, KeepIfZero
    pub cmd: String,
}
```

The keyboard verbs (`|`, `Alt-|`, `!`, etc.) still go through the
prompt → run flow; the palette verbs skip the prompt and call
`vim::ShellRun { mode, cmd }` directly.

## Why this clause

The palette path is what Helix users reach for when they want to
type the command rather than re-prompt — `:pipe sort | uniq` is
faster than `|`-then-prompt for the same operation. The `:sh`
verb is the one Helix surface that has no keyboard counterpart and
needs a place to live in codon — palette is the natural home.

The "transient buffer pane" for `:sh` output is also useful in its
own right: it's the closest thing codon has to a one-shot
terminal output, without spinning up a full terminal pane.

## Verification

- Open `:`. Type `pipe sort | uniq`. Confirm. Selection-replace
  flow runs the same as keyboard `|` with the command preserved.
- Open `:`. Type `sh ls -la`. Confirm. New transient buffer pane
  appears with the directory listing.
- Open `:`. Type `keep-pipe grep -q TODO`. Confirm. Multi-cursor
  narrowing same as keyboard `$`.
- Tab-completion (if the palette supports it) shows the five
  verbs.
- Cheatsheet doesn't list these (palette verbs aren't bindings).

## Done when

- Five palette verbs registered.
- `:sh` opens a transient output buffer pane.
- The four selection-aware verbs reuse the vim crate's
  `ShellRun(mode, cmd)` handler (single code path).
- A unit test verifies palette parsing — `:pipe sort | uniq`
  yields `(mode=PipeReplace, cmd="sort | uniq")`.
- `cargo clippy` reports no warnings.
- `spec lint` is at zero errors.
