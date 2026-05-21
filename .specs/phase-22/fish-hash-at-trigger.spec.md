---
id: TASK:phase-22/fish-hash-at-trigger
type: task
status: accepted
version: 0.1.0
summary: >
  The `#@` parse + `Ctrl-G` trigger flow: capture the buffer,
  split on the last `#@`, send to `agent.complete`, replace the
  buffer with the suggestion. Cancellation via `Ctrl-C`. No
  auto-execute — Enter remains the user's keystroke.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-magic-hash-at-syntax
  - REQ:codon/fish-shell-integration#c-magic-hash-at-trigger
  - REQ:codon/fish-shell-integration#c-magic-hash-at-flow
  - REQ:codon/fish-shell-integration#c-no-auto-execute
  - REQ:codon/fish-shell-integration#c-cancellation
  - REQ:codon/fish-shell-integration#c-shell-syntax-output
aspects: [parse, trigger-binding, async-flow, cancellation, no-execute, shell-syntax]
blocked_by:
  - TASK:phase-22/fish-rpc-socket
  - TASK:phase-22/harness-api
---

# `#@` parse + Ctrl-G trigger + buffer rewrite

## Plan

- Fish-side parser (in `codon.fish`):
  - `__codon_parse_hash_at` reads `commandline -b` (full
    buffer) and returns two strings:
    - `partial` = bytes before the LAST `#@` (trimmed of
      trailing whitespace).
    - `description` = bytes after the LAST `#@` (trimmed of
      leading whitespace).
    - If no `#@` is present: `partial = ""`,
      `description = $buffer`.
  - Earlier `#@` occurrences are treated as literal text.
    Document this in the plugin's help.
- Trigger binding:
  - `bind \cg __codon_hash_at_trigger` installed only when
    `set -q CODON_SOCK`.
  - The chord is configurable: at plugin source time, read
    `[fish_integration] trigger_key` from `codon.toml` via
    `__codon_rpc "config.get" { key: "fish_integration.trigger_key" }`
    with a 200 ms timeout. Fall back to `\cg` on timeout.
- The trigger handler:
  1. Save the original buffer in a script-local variable.
  2. Replace the buffer with `… asking codon agent (Ctrl-C
     to cancel)` using `commandline --replace`. The
     replacement is rendered with a dim color via
     `set_color brblack` for visibility.
  3. Spawn an async `__codon_rpc` call to `agent.complete`
     with `{ partial, description, cwd: (pwd), shell:
     "fish" }`. (Context injection is the next sibling task.)
  4. Install a one-shot `Ctrl-C` handler that cancels the
     RPC and restores the original buffer.
  5. On RPC success with a `SuggestCommand`:
     - `commandline --replace -- $command`.
     - `commandline -f end-of-line` to put the cursor after
       the rewrite.
     - The user owns Enter from here.
  6. On RPC success with a `SuggestResponse` (model couldn't
     produce a command): print the response to stderr above
     the prompt; restore the original buffer.
  7. On RPC error (timeout, redactor-Risky,
     network-failure): print a one-line stderr message;
     restore the buffer.
- Server side:
  - `agent.complete` builds a `Harness::run_turn` call with a
    `FishCompleteFlow` shape that locks the available tools:
    - Tools enabled: `read_current_pane`, `grep_pane`,
      `list_panes`, `search_command_history` (when
      enabled), `search_memories`.
    - Tools forbidden: `suggest_action` (no action
      dispatch from this flow), `suggest_response` and
      `suggest_command` are the only allowed reply shapes.
  - The system prompt instructs the model: "the target
    shell is fish; emit fish-syntax (functions, `set`,
    `(...)` for subshells, no `$((...))`, no `&&` chains
    over multiple lines without continuation)."
  - The model's `suggest_command` reply is the response
    body. `suggest_response` is reserved for the "no clean
    command" fallback path.
- No auto-execute test: a stub `__codon_rpc` returning
  `{ command: "rm -rf /" }` produces a buffer-replace but
  NEVER triggers `commandline -f execute`. Asserted in unit
  tests against a fish-in-a-pty test harness.

## Acceptance

- `git checkout #@ the auth branch` + Ctrl-G triggers the
  RPC, replaces the buffer with something like `git
  checkout feat/auth-middleware-rewrite`. Enter executes;
  the user-owned keystroke is mandatory.
- `#@ install rust toolchain 1.95` + Ctrl-G produces a
  command from scratch.
- Ctrl-C while "asking…" cancels the RPC, restores the
  original buffer, harness trace shows the turn as
  `Cancelled`.
- The `__codon_parse_hash_at` parser handles edge cases:
  no `#@`, multiple `#@`s, `#@` at start of buffer, `#@`
  at end (empty description).
- No-execute test: instrumented stub RPC returning a
  command never causes execution.
- `cargo test -p codon-fish` passes.
