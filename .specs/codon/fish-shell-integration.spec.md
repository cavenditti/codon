---
id: REQ:codon/fish-shell-integration
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  First-class fish-shell interop: a shell-side companion plugin
  that talks to the running codon instance over a per-workspace
  Unix socket. Two surfaces — `codon do <action>` for dispatching
  any codon action from the shell, and `#@` magic completion for
  generating / completing commands with the agent inline in the
  shell buffer. Both go through the existing harness; both honour
  the no-auto-execute invariant; both degrade silently when codon
  isn't the parent.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Fish shell integration

## Context

Codon's terminals are the primary work surface. The phase-22 agent
work made the codon side capable of inspecting and suggesting; this
REQ closes the loop in the other direction — fish, the shell
running inside the terminal, becomes a first-class peer that can
both *trigger codon actions* and *invoke the agent inline in the
shell prompt*. Two scenarios drive the requirement:

1. **Dispatching codon from the shell.** The user is mid-command
   in a terminal pane and wants to "open this file in a split"
   or "toggle hidden files in the FM" or "go to window 3" without
   leaving the keyboard. Today the path is "use a keybinding from
   the terminal pane". That works for global verbs but breaks for
   verbs whose arguments are paths the shell already has resolved
   (`codon edit ./src/foo.rs` is one keystroke; binding it to a
   chord plus an argument prompt is three).
2. **Agent-completed commands.** The user types a shell command
   and either (a) tags a natural-language description after `#@`
   to complete it, or (b) starts the buffer with `#@` to generate
   one from scratch. Examples:
   - `git checkout #@ the branch with the auth refactor commits`
   - `find . -name '*.rs' #@ but only files modified this week`
   - `#@ install the rust toolchain pinned in rust-toolchain.toml`
   On a trigger keystroke (default `Ctrl-G`) the shell consults
   codon's agent, gets back a single command, and replaces the
   buffer with it. Enter executes; the user can edit first.
   The trigger never auto-executes.

Both surfaces share one substrate:

- **Per-workspace Unix socket.** Codon advertises a socket path
  via `CODON_SOCK` in every PTY's environment. The fish plugin
  reads `$CODON_SOCK`, opens the socket, and stays connected for
  the shell's lifetime.
- **JSON-line RPC over the socket.** Compact, easy to debug with
  `socat`. Codon-side handler dispatches into the existing action
  registry (for `codon do`) or the harness (for `#@`).
- **Same harness, same redaction, same trace.** A `#@` invocation
  is a regular `Harness::run_turn` call returning a
  `SuggestCommand` shape. The fish plugin renders the result;
  codon's contextual-suggest verb stays the GUI-side equivalent.

Three invariants:

1. **No auto-execute.** Ever. `#@` rewrites the shell buffer; the
   user owns Enter.
2. **Codon-absent → silent.** Fish without `CODON_SOCK` set is
   regular fish. Every plugin feature checks for the socket and
   no-ops in its absence. `codon do <anything>` outside codon
   prints a one-line message to stderr explaining the
   requirement; `#@` and `Ctrl-G` simply do nothing (the binding
   is not installed when `CODON_SOCK` is unset). The user can
   take their dotfiles to a non-codon terminal and the only
   change is that those features disappear.
3. **Fish first; bash + zsh deferred.** Phase 22 ships the fish
   plugin only. Bash and zsh are explicit follow-ups — same
   RPC, different plugin language — with a clean factor-out
   already considered in the protocol design.

:::{requirement id="fish-shell-integration" level="MUST"}
The system MUST provide:

- {#c-rpc-socket} a per-workspace Unix-domain socket whose path
  is exported as `CODON_SOCK` in every PTY environment codon
  spawns. Socket path:
  `$XDG_RUNTIME_DIR/codon/<fingerprint>-<window_id>.sock`
  (falls back to `$TMPDIR/codon-...` on macOS). The socket is
  created on workspace open, removed on close. Multiple
  windows of the same workspace get distinct sockets — each
  RPC call routes to the originating window
- {#c-rpc-protocol} the RPC protocol is JSON-line over the
  socket:
  - Client → server: one JSON object per line with `{ id,
    method, params }` (subset of JSON-RPC 2.0; no batching, no
    notifications).
  - Server → client: `{ id, ok: <result> }` or `{ id, err: {
    code, message } }`. Streaming responses (for `#@` while
    the agent is thinking) use multiple lines per id with a
    final `done: true` marker.
  - Methods: `action.dispatch`, `agent.complete`,
    `terminal.context`, `health.ping`. Versioning via a
    `version` field on the initial handshake message
- {#c-action-dispatch} `codon do <action_name> [json-args]` is a
  fish function that sends an `action.dispatch` RPC. The
  resolver:
  1. Look up the action in the global registry by typed name
     (same registry the TOML keymap uses).
  2. Dispatch on the action's previously-focused pane (codon
     records "the pane the terminal lives in" at PTY spawn).
  3. Return the action's outcome (`ok` or `err { unknown |
     payload_invalid }`).
  Tab completion in fish for `codon do <Tab>` returns the
  current action registry — implemented by a small fish
  completion script that calls `codon do --__complete` and
  caches the result per session
- {#c-action-convenience-helpers} a small set of convenience
  fish functions wrap `codon do` for the common cases:
  - `codon edit <path...>` → `editor::OpenFile { path }` per
    arg, focuses the resulting pane
  - `codon split [right|down|left|up]` →
    `codon_session::SplitTerminal*` depending on direction
  - `codon win <n|next|prev>` →
    `codon_session::Window*` with the obvious mapping
  - `codon fm [path]` → `file_manager::OpenAt { path }`
    (default cwd)
  Each helper is a thin shim over `codon do`; the long form
  always works
- {#c-magic-hash-at-syntax} the fish plugin recognises `#@`
  as the agent-trigger sentinel in the command-line buffer.
  Two parse shapes:
  - `<partial> #@ <description>` — `<partial>` is the prefix
    the agent must extend; `<description>` is the NL
    instruction. Example: `git checkout #@ the branch with
    the auth changes` → "complete the args for `git
    checkout`, branch matching the description".
  - `#@ <description>` — no prefix; agent generates from
    scratch. Example: `#@ install the rust toolchain pinned
    in rust-toolchain.toml` → "produce the command".
  - Multiple `#@` markers in the same line: only the LAST
    `#@` splits; earlier occurrences are treated as literal
    shell comments (which they are once the line executes)
- {#c-magic-hash-at-trigger} a fish keybinding (default
  `Ctrl-G`, configurable via
  `[fish_integration] trigger_key = "ctrl-g"` in
  `codon.toml`) fires the agent-complete flow. The binding
  is installed by the plugin only when `CODON_SOCK` is set;
  without codon the keystroke remains free
- {#c-magic-hash-at-flow} on trigger:
  1. The plugin captures `commandline -b` (the full buffer)
     and parses out `<partial>` + `<description>` per the
     `#@` syntax. If `#@` is absent, the trigger sends the
     whole buffer as `<description>` with empty `<partial>`
     (the user gets an "I assume the whole line is intent"
     fallback).
  2. The plugin replaces the buffer with a temporary
     read-only placeholder: `… asking codon agent …`.
     `Ctrl-C` (also bound by the plugin while waiting)
     restores the original buffer.
  3. Plugin sends `agent.complete { partial, description,
     cwd, shell: "fish", recent_commands: <last 5 from the
     in-shell history, no codon-history access here> }`
     over the socket.
  4. Codon's handler builds the agent prompt, runs through
     the redaction pipeline at egress, calls
     `Harness::run_turn` with a synthetic flow that locks
     down the available tools to read-only inspection + the
     `suggest_command` shape only.
  5. Response comes back as a single command string. The
     plugin replaces the buffer with the command (no
     trailing newline; the user owns Enter).
  6. If the agent returns no plausible command (the
     `SuggestResponse` fallback fires instead), the plugin
     restores the original buffer and prints the response
     text to stderr above the prompt
- {#c-no-auto-execute} the agent's reply is NEVER executed
  automatically. The plugin uses `commandline --replace`
  followed by `commandline -f end-of-line` to position the
  cursor at the end of the new buffer; Enter remains the
  user's keystroke. A unit test against a stub plugin
  asserts the buffer-replace pathway never calls
  `commandline -f execute`
- {#c-context-injection} the `agent.complete` handler
  enriches the prompt with: the focused terminal's cwd, the
  pane-kind preamble snippet (from
  REQ:codon/agent-context-preamble), the project-kb
  directory summary (from
  REQ:codon/project-knowledge-base, when available), and
  the last 20 entries from REQ:codon/command-history
  whose cwd matches (when the feature is enabled). All
  context goes through the redaction pipeline before
  reaching the model
- {#c-graceful-degradation} every fish-side feature checks
  `set -q CODON_SOCK` before activating:
  - The `Ctrl-G` binding is installed only when set.
  - `codon do` outside codon prints
    `codon: not running (CODON_SOCK unset)` to stderr,
    exit 1. No partial behaviour.
  - The plugin's `fish_prompt`-time hooks no-op cleanly.
  A user sourcing the plugin from `~/.config/fish/conf.d/`
  on a non-codon terminal experiences zero side-effects
- {#c-plugin-distribution} the fish plugin lives at
  `crates/codon-fish/share/codon.fish` (the canonical
  source), and `codon fish-init` is a CLI subcommand that
  writes it to
  `~/.config/fish/conf.d/codon.fish` with idempotent
  semantics (overwrite only if the user has not modified
  the file since the last write — checksum stored alongside).
  `codon fish-init --uninstall` removes the file and the
  checksum. The first time a user opens a terminal in codon
  without the plugin installed, codon shows a one-line toast
  with the command to install
- {#c-shell-syntax-output} the agent receives the active
  shell ("fish" in this REQ) and MUST emit shell-syntactic
  output. The system prompt for the `#@` flow includes
  `target shell: fish; produce fish-syntax (functions, set,
  -- not bash $((...)) arithmetic, etc.)`. Cross-shell
  syntax mistakes show up in the trace as model-quality
  feedback; the plugin never tries to translate
- {#c-cancellation} `Ctrl-C` while the agent is computing
  cancels the harness turn (same `CancelToken` path as
  every other harness flow) and restores the buffer. The
  plugin's cancel handler is installed only for the
  duration of the in-flight request — outside that window,
  `Ctrl-C` is fish's normal behaviour
- {#c-redact-at-egress} the prompt assembled by the
  handler MUST go through the redaction pipeline at
  egress. The user's `<description>` text is also
  redacted — they could paste a token into the NL field
  by accident. A `Risky` outcome aborts: the plugin gets
  an error response, restores the buffer, and prints
  `codon: agent declined (sensitive content)` to stderr.
  The trace records the redaction event
- {#c-discoverability} `codon help` and `codon do --help`
  list the action-dispatch surface. `codon do <Tab>` in
  fish tab-completes to the action registry. The
  cheatsheet (`cmd-k F1`) has a new "Shell" section
  documenting `codon do`, `#@`, `Ctrl-G`, and the
  convenience helpers
:::

## Out of scope

- **Bash and zsh.** Deferred. The RPC is shell-agnostic; a
  bash/zsh plugin is a follow-up REQ. Phase-22 ships fish only.
- **Cross-shell command translation.** Codon doesn't try to
  rewrite a bash-syntax suggestion into fish syntax. The system
  prompt instructs the model to emit the target shell's syntax;
  if it doesn't, the user edits before Enter.
- **Auto-execute.** Hard non-goal. No flag, no preference, no
  exception. The user owns Enter.
- **Shell scripting.** This REQ targets interactive shells. A
  user invoking `codon do` from a `.fish` script works, but
  `#@` requires the interactive line editor and is a no-op in
  non-interactive shells.
- **Remote codon (SSH).** If the user `ssh` into a remote box
  from inside a codon terminal, `CODON_SOCK` is not forwarded
  by default. The remote fish degrades cleanly. A "forward the
  socket over ssh" mode is a separate piece of plumbing,
  deferred.
- **Auto-suggestion of `#@` completion as you type.** The
  trigger is explicit (`Ctrl-G`) because every agent call costs
  tokens. An "as-you-type" version is a future opt-in if a use
  case emerges.
