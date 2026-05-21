---
id: TASK:phase-22/fish-rpc-socket
type: task
status: accepted
version: 0.1.0
summary: >
  Per-window Unix-domain socket + JSON-line RPC server lifecycle.
  Codon spawns a socket on workspace open, removes it on close,
  injects `CODON_SOCK` into every PTY environment, dispatches
  incoming RPC messages to the action registry or the harness.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-rpc-socket
  - REQ:codon/fish-shell-integration#c-rpc-protocol
aspects: [socket-lifecycle, json-line-protocol]
---

# RPC socket + protocol

## Plan

- New crate `crates/codon-fish/` with lib-root
  `src/codon_fish.rs`. Despite the name, the crate hosts the
  *codon-side* RPC; the fish plugin file is a sibling artifact
  under `crates/codon-fish/share/`.
- Socket lifecycle:
  - On workspace open + per-window: pick a path
    `$XDG_RUNTIME_DIR/codon/<fingerprint>-<window_id>.sock`
    (fallback `$TMPDIR/codon-<fp>-<wid>.sock` on macOS where
    XDG_RUNTIME_DIR isn't standard). Create the parent dir with
    `0700`; bind the `UnixListener`.
  - On window close / workspace close: drop the listener and
    `unlink` the socket. A guard in `Drop` handles panics too.
  - Stale-socket recovery: at startup if the path exists, try
    `connect` first; if no one's listening, `unlink` and rebind.
    Documented behaviour because crashed-codon-leaves-socket is
    a real failure mode.
- Env injection:
  - At PTY spawn (codon's terminal-pane setup; see
    [vendor/zed/crates/terminal](spec:src:vendor/zed/crates/terminal/src/terminal.rs)
    surroundings), set `CODON_SOCK = <path>` plus
    `CODON_WINDOW = <window_id>` in the child environment.
  - The window id lets the plugin attribute the RPC to the
    right pane chain when codon's handler resolves "where to
    dispatch".
- Protocol (JSON-line, one object per `\n`-terminated line):
  - Handshake: client sends `{ id: 0, method: "hello",
    params: { plugin_version: "...", client: "fish" } }`.
    Server replies `{ id: 0, ok: { server_version: "...",
    methods: ["action.dispatch", "agent.complete", "terminal.
    context", "health.ping"] } }`. Version negotiation:
    incompatible versions get a `version_unsupported` error
    and the plugin downgrades to no-op mode.
  - Request/response: `{ id, method, params }` →
    `{ id, ok }` or `{ id, err: { code, message } }`.
  - Streaming (used by `agent.complete` to surface
    "thinking…"): multiple `{ id, partial: {...} }` lines
    followed by a final `{ id, ok | err, done: true }`.
- Dispatch routing:
  - `action.dispatch` → look up by typed action name in Zed's
    global registry, dispatch on the recorded "PTY's owning
    pane".
  - `agent.complete` → spawn an async task that calls into
    `codon_agent::Harness::run_turn`. Returns a `SuggestCommand`
    or `SuggestResponse`.
  - `terminal.context` → returns the PTY's cwd / shell / last
    OSC 133 prompt info for the plugin's context-injection
    needs.
  - `health.ping` → returns `{ ok: { ts } }`. Used by the
    plugin's reconnect loop.

## Acceptance

- Opening a workspace creates the socket at the documented
  path with `0700` parent dir.
- A test harness connecting via `UnixStream` can do a `hello`
  handshake.
- Killing codon -9 leaves a stale socket; a subsequent codon
  start unlink-rebinds cleanly (test via spawn → kill →
  respawn).
- `CODON_SOCK` is set in every PTY codon spawns; verified by
  an integration test that reads `env` inside a synthetic
  terminal.
- `cargo test -p codon-fish` passes.
