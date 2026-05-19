---
id: REQ:codon/terminal-blocks
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Terminal panes expose command-and-output as a typed `Block`
  object — detected via OSC 133 prompt markers when the user's
  shell emits them, falling back to heuristic boundary detection
  from the terminal byte stream — so block-aware navigation,
  re-run, and cross-pane verbs (`codon_agent::Explain`) work
  uniformly with the rest of codon's object-verb grammar.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-19]
---

# Terminal blocks as typed objects

## Context

Codon's terminal pane today exposes raw text selection (alacritty's
grid + vi mode) but no notion of *block* — the Warp/Wave insight
that a single shell command together with its stdout / stderr is
one logical unit. Without a block object kind, the most-promised
Section 6 workflow — "select the error block, send to agent, fix"
— is a manual copy-paste loop.

The cheap path is OSC 133, an emerging shell-integration sequence
(`\e]133;A\e\\` before prompt, `\e]133;B\e\\` before command,
`\e]133;C\e\\` before output, `\e]133;D;<exit>\e\\` after output)
already supported by zsh-defer, fish, kitty, wezterm, ghostty. The
fallback path is heuristic prompt-boundary detection — scan the
terminal byte stream for prompt-shaped lines — lossy but works
without shell cooperation.

Both paths feed the same `Block` object kind; `SelectionSource`
for terminal panes returns `Selection::Blocks(...)` once a block
is selected.

:::{requirement id="terminal-blocks" level="MUST"}
The system MUST provide:

- {#c-block-object} a `Block` typed object holding
  `command: String`, `output: String`, `exit_status: Option<i32>`,
  `start: alacritty::Anchor`, `end: alacritty::Anchor`, plus the
  source attribution (`Detection::Osc133 | Detection::Heuristic`).
  The `ObjectKind::Block` variant in `codon_pane_bridge::Selection`
  carries `Vec<TerminalBlockRef>` referencing blocks by terminal
  pane id + index.

- {#c-osc-133-parser} OSC 133 sequence parsing in the vendored
  `alacritty_terminal` event stream. Codon registers a listener on
  the existing escape-code handler; sequences are converted to
  `BlockBoundary { kind: PromptStart | CommandStart | OutputStart |
  OutputEnd { exit }, anchor }` events. A `BlockStore` per
  terminal-pane entity reassembles boundaries into `Block` records.
  Out-of-order or partial sequences (e.g. `D` without `C`) degrade
  gracefully — the partial block is dropped, the store keeps
  scanning for the next `A`.

- {#c-heuristic-detector} a prompt-pattern heuristic detector that
  runs when no OSC 133 sequence has been seen in the last N lines
  (default 200). It scans new lines for prompt-shaped prefixes
  (configurable regex set, defaults shipping with zsh / bash / fish
  patterns) and synthesises `BlockBoundary` events. Heuristic
  blocks have no exit status (`exit_status: None`) and carry
  `Detection::Heuristic`. Detector confidence is exposed so the
  status bar can render heuristic blocks dimmer than OSC 133 ones.

- {#c-shell-integration-snippets} opt-in shell snippets shipped
  under `assets/shell/codon-osc133.{zsh,bash,fish}` that emit the
  four OSC 133 sequences around prompts and commands. The
  command palette exposes `codon_terminal::InstallShellIntegration`
  which prints the appropriate `source` line for the user's
  `$SHELL` plus a one-line copy-to-clipboard prompt — no automatic
  rc-file edits.

- {#c-selection-source} the terminal pane's `SelectionSource` impl
  returns `Selection::Blocks(...)` whenever the current selection is
  a block selection, `Selection::Text { ... }` for a raw character
  selection. A pane can hold at most one selection mode at a time —
  block-selecting clears any character selection and vice versa.

- {#c-navigation} terminal Normal mode adds block-aware motion:
  `]b` next block, `[b` previous block, `]B` last block, `[B` first
  block, `mib` select inner block (command + output), `mab` select
  around block (include prompt line), `%b` select all blocks in the
  scrollback. Bindings live in the embedded TOML defaults under
  `[bindings.terminal.normal]` so they're keymap-overridable.

- {#c-cross-pane-verbs} the existing cross-pane verbs accept
  `ObjectKind::Block` where it makes semantic sense:
  `codon_agent::Explain` ([Text, Hunk, Block, Diagnostic, Message]),
  `codon_agent::Fix` ([Diagnostic, Block] — block = error output),
  `codon_agent::Summarize` ([Text, Block, Message]). A new verb
  `codon_terminal::RerunBlock` accepts `[Block]` only — re-types
  the selected block's command into the terminal as if the user
  pressed up-arrow and Enter.

- {#c-status-bar} a status-bar indicator (right zone) shows the
  current block detection mode for the focused terminal pane:
  `osc133` (green), `heuristic` (yellow), `none` (dim). Click /
  press-binding opens the shell-integration installer.

- {#c-persistence} block records are *not* persisted across
  restarts — they are derived from the scrollback, which itself
  isn't persisted in PTY-form. On rehydrate the heuristic detector
  re-scans the last-saved scrollback chunk; OSC 133 blocks only
  exist for the live session.
:::

## Out of scope

- Inline block "fold" UI in the terminal pane (the Warp/Wave
  fold/expand indicator). The selection model is enough; folding
  is a follow-on phase.
- OSC 133 sequences from remote SSH sessions — out of scope until
  the deferred SSH-remote work lands.
- Rerunning block commands in a *different* terminal than the
  source. `RerunBlock` types into the same pane.
