---
id: REQ:codon/shell-integration
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Wire Helix's five shell verbs (`|` pipe-selection-through,
  `Alt-|` pipe-to, `!` insert-output-before, `Alt-!`
  append-output-after, `$` keep-pipe) as editor actions in the
  vendored vim crate plus a matching codon-command-palette path
  (`:sh`, `:pipe`, `:insert-output`, `:append-output`,
  `:keep-pipe`). Pipe commands feed each multi-cursor selection
  through `$SHELL -c <cmd>` on stdin and consume stdout per
  Helix semantics. Output target is the editor buffer (for
  pipe/insert/append) or the selection set (for keep-pipe).
owners: [carlo]
categorized_under: [TOPIC:topics/phase-16]
---

# Helix shell-pipe integration

## Context

Helix's killer feature on the editing side is its shell
integration. Five verbs in normal/select mode:

- `|` — pipe each selection through `$SHELL -c <cmd>`, replace each
  selection with the command's stdout.
- `Alt-|` — pipe each selection through, ignore output.
- `!` — run `$SHELL -c <cmd>`, insert stdout *before* each
  selection.
- `Alt-!` — run `$SHELL -c <cmd>`, append stdout *after* each
  selection.
- `$` — pipe each selection through, keep selections whose command
  returned exit code 0.

Vendored Zed has no equivalent action — Zed's vim crate has plenty
of editor verbs but no "pipe selection through shell" surface.
Codon is the natural home: terminal panes are a first-class
concept, the `:` palette already exists, and `codon-pickers`
already supplies `ModalScaffold` for one-line prompts (the
prompt-for-command flow is the same shape as the existing
`SessionRename` modal).

Two entry points, one runtime:

1. **Keyboard.** `|` `!` `$` `Alt-|` `Alt-!` under
   `vim_mode == helix_normal || helix_select` in
   `[bindings.editor.normal]` of codon's `DEFAULT_KEYMAP`.
2. **Palette.** `:sh <cmd>` / `:pipe <cmd>` /
   `:insert-output <cmd>` / `:append-output <cmd>` /
   `:keep-pipe <cmd>` via a `codon-command-palette::completer`
   variant that accepts free-form text rather than a fuzzy match.

The shell shells out to `$SHELL -c <cmd>` (or `/bin/sh -c <cmd>` if
`$SHELL` is unset). Codon never inspects `<cmd>` — the user's shell
is responsible for quoting, expansion, and PATH.

:::{requirement id="shell-integration" level="MUST"}
Shell integration MUST:

- {#c-actions-five} register five vim crate actions:
  `vim::ShellPipeSelection`, `vim::ShellPipeTo`,
  `vim::ShellInsertOutput`, `vim::ShellAppendOutput`,
  `vim::ShellKeepPipe`. Each takes no payload; the command is
  prompted at runtime.
- {#c-prompt-ux} open a 1-line prompt (built on
  `codon-pickers::ModalScaffold`) when the action fires, prefilled
  with the action's mnemonic (`pipe>`, `insert>`, `append>`,
  `keep>`, `pipe-to>`). Enter confirms; escape cancels.
- {#c-multi-cursor} for each Helix selection, spawn an OS process
  (`std::process::Command`), pass the selection's text to stdin,
  and read stdout into a string. Run all selections in parallel
  (bounded concurrency) and apply all results atomically.
- {#c-replace-semantics} for `ShellPipeSelection`, replace each
  selection with its stdout, preserving selection bounds.
- {#c-pipe-to-semantics} for `ShellPipeTo`, discard stdout, leave
  the buffer unchanged.
- {#c-insert-semantics} for `ShellInsertOutput`, insert stdout
  before each selection start.
- {#c-append-semantics} for `ShellAppendOutput`, append stdout
  after each selection end.
- {#c-keep-pipe-semantics} for `ShellKeepPipe`, keep only those
  selections where the spawned process exited with code 0; drop
  the rest. No text edits.
- {#c-stderr-routing} on any non-zero exit, surface a toast with
  the first ~120 chars of stderr and leave the buffer unchanged.
  Multi-cursor: surface a single toast summarizing the failure
  count (`pipe: 2 of 5 selections failed: …`).
- {#c-palette-verbs} register five palette verbs (`sh`, `pipe`,
  `insert-output`, `append-output`, `keep-pipe`) via
  `codon-command-palette::completer::Completer`. The `sh` verb is
  the standalone "run a command, capture output to a transient
  buffer" form Helix exposes as `:sh` (no selection involvement).
- {#c-binding-defaults} bind in `DEFAULT_KEYMAP` under
  `[bindings.editor.normal]`:
  - `|` → `vim::ShellPipeSelection`
  - `alt-|` → `vim::ShellPipeTo`
  - `!` → `vim::ShellInsertOutput`
  - `alt-!` → `vim::ShellAppendOutput`
  - `$` → `vim::ShellKeepPipe`
- {#c-shell-env} use `$SHELL` if set and non-empty, falling back
  to `/bin/sh`. Pass `-c <cmd>`. The shell command's cwd is the
  project's first worktree root (matches codon-session's
  pane-seeding convention).
- {#c-timeout-safety} kill the spawned process after a configurable
  timeout (`[shell] timeout_ms = <int>` in `codon.toml`, default
  5000). Killed processes count as failed for keep-pipe.
- {#c-no-shell-in-vendored-zed-keymap} the bindings live in codon
  TOML, not in `vendor/zed/assets/keymaps/vim.json`. Vim crate
  changes are limited to action registration + handler
  implementations.
:::

## Why this REQ

Shell pipe is Helix's most-asked-about feature among new users and
the one Zed-native users are most surprised codon doesn't have.
Codon's terminal-pane substrate makes it the easier place to land
than vanilla Helix — the toast surface, the transient prompt, and
the codon-pickers scaffold all already exist. Keeping the bindings
in codon TOML (not vim.json) honors the project invariant that
TOML is the single source of truth.

## Done when

- The five `vim::Shell*` actions are registered and handled.
- The prompt modal is reusable from both the keyboard verbs and
  the palette verbs.
- Five palette verbs work in the `:` palette with free-form
  command input.
- The bindings live in codon `DEFAULT_KEYMAP`, not vim.json.
- A clean cycle of `cargo clippy` + `vendor/zed/script/clippy`
  reports no warnings.
- `spec lint` is at zero errors.
