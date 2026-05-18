---
id: TASK:phase-16/shell-pipe-action
type: task
status: draft
version: 0.0.1
summary: >
  Register `vim::ShellPipeSelection` and `vim::ShellPipeTo` in
  vendored Zed's vim crate. Both prompt for a shell command, spawn
  `$SHELL -c <cmd>` per selection, and pipe the selection's text
  through stdin. PipeSelection replaces each selection with the
  command's stdout; PipeTo discards stdout. Bound to `|` and
  `alt-|` in codon TOML.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/shell-integration#c-actions-five
  - REQ:codon/shell-integration#c-prompt-ux
  - REQ:codon/shell-integration#c-multi-cursor
  - REQ:codon/shell-integration#c-replace-semantics
  - REQ:codon/shell-integration#c-pipe-to-semantics
  - REQ:codon/shell-integration#c-stderr-routing
  - REQ:codon/shell-integration#c-binding-defaults
  - REQ:codon/shell-integration#c-shell-env
  - REQ:codon/shell-integration#c-timeout-safety
  - REQ:codon/shell-integration#c-no-shell-in-vendored-zed-keymap
aspects: [actions, prompt, multi-cursor, replace, pipe-to, stderr, bindings, shell-env, timeout, toml-only]
---

# Shell pipe actions (`|` and `Alt-|`)

## What changes

Add `vendor/zed/crates/vim/src/shell.rs` (new file, follows the
no-mod.rs convention in
[`vendor/zed/CLAUDE.md`](spec:src:vendor/zed/CLAUDE.md)):

```rust
actions!(
    vim,
    [
        /// Prompt for a shell command; pipe each selection's text
        /// through `$SHELL -c <cmd>`; replace each selection with
        /// the command's stdout. Non-zero exit → toast + skip.
        ShellPipeSelection,
        /// Prompt for a shell command; pipe each selection's text
        /// through `$SHELL -c <cmd>`; discard stdout. Used for
        /// side-effect commands (e.g. `pbcopy`, `notify-send`).
        ShellPipeTo,
        // ... (other three actions live in sibling tasks)
    ]
);
```

Registration in `crates/vim/src/vim.rs::register` (or wherever the
crate's master action register lives — codon convention puts the
helix action registration in `helix.rs::register`; the shell
analog goes alongside in `shell.rs::register(editor, cx)`).

Implementation skeleton:

```rust
pub fn shell_pipe_selection(
    vim: &mut Vim,
    _: &ShellPipeSelection,
    window: &mut Window,
    cx: &mut Context<Vim>,
) {
    open_shell_prompt(
        vim,
        ShellPromptKind::Pipe,
        window,
        cx,
        |vim, cmd, window, cx| {
            run_for_each_selection(
                vim, cmd, ShellMode::ReplaceWithStdout, window, cx,
            );
        },
    );
}
```

- `open_shell_prompt` is a shared helper in `shell.rs` that opens
  a 1-line input modal (built on
  [`codon-pickers::ModalScaffold`](spec:src:crates/codon-pickers/);
  vim crate cannot depend on a codon crate without inverting
  layering — instead, expose the prompt as a *workspace-level
  modal* from `codon-pickers`, and have the vim action emit a
  workspace action `codon_pickers::ShellPrompt(ShellPromptKind)`
  that codon-pickers handles. The vim crate stays free of codon
  deps.

  **Plan revision**: the vim crate registers the *action*, but the
  *prompt UI* is hosted on the workspace by codon-pickers. The
  vim action's handler builds the per-selection text snapshot and
  emits a `WorkspaceShellInvocation` event the codon-pickers
  workspace handler consumes; the handler opens the prompt and on
  confirm calls back into a `vim::execute_shell_pipe(snapshot, cmd, mode)`
  free function. Cleaner layering; same behavior.

- `run_for_each_selection`:
  - Capture each selection's text via
    `Editor::selections_text(&buffer, cx)`.
  - For each selection, spawn `$SHELL -c <cmd>` (or `/bin/sh -c
    <cmd>` if `$SHELL` is empty) with stdin = selection text.
  - Bound concurrency to `num_cpus()` or 8 (whichever is smaller).
  - Apply a `timeout_ms` (default 5000) kill.
  - Collect per-selection results: `(exit_code, stdout, stderr)`.
  - Apply all replacements in one `editor.transact(cx, ...)` so
    undo treats them atomically.

- Stderr routing per `REQ:codon/shell-integration#c-stderr-routing`:
  collect non-zero results, surface a single toast with the
  failure count + first stderr's leading 120 chars.

Codon binding in `DEFAULT_KEYMAP`:

```toml
[bindings.editor.normal]
"|"     = "vim::ShellPipeSelection"
"alt-|" = "vim::ShellPipeTo"
```

Note: vim.json must *not* receive these bindings — they live in
codon TOML only, per
`REQ:codon/shell-integration#c-no-shell-in-vendored-zed-keymap`.
The `editor.normal` predicate resolves to
`vim_mode == normal || helix_normal || helix_select`; the vim
crate's helix-mode handlers will route the `|` keystroke to the
new action.

## Why this clause

This task delivers the first two of the five shell verbs in one
slice. They share infrastructure (prompt UX, per-selection
spawn, concurrency, timeout, toast). The remaining three verbs
([`shell-output-actions`](spec:.specs/phase-16/shell-output-actions.spec.md),
[`shell-keep-pipe`](spec:.specs/phase-16/shell-keep-pipe.spec.md))
re-use the same helpers and add only the
output-application semantics.

## Verification

- Open a markdown file. Select three list items. Press `|`. Type
  `sort | uniq`. Selections replaced with sorted output.
- Select a JSON blob. `|` → `jq .`. Selection replaced with
  pretty-printed JSON.
- Select a region. `Alt-|` → `pbcopy` (macOS). Selection
  unchanged; clipboard now holds the text.
- Pass a failing command (`|` → `false`). Toast appears; buffer
  unchanged.
- Pass a slow command (`|` → `sleep 10`). After 5 s, process is
  killed, toast appears.
- Undo. All replacements revert atomically.

## Done when

- `vim::ShellPipeSelection` and `vim::ShellPipeTo` are registered
  and bound in codon TOML.
- Multi-cursor pipe applies in parallel, atomic transact.
- Timeout kills runaway commands.
- Toast surfaces stderr count.
- A unit test on `run_for_each_selection` (or a small integration
  test using `cat` and `false`) covers replace + skip-on-error +
  timeout.
- `vendor/zed/script/clippy` reports no new warnings.
- `spec lint` is at zero errors.
