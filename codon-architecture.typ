// Codon — Architecture and Implementation Plan
// Design document v0.1

#set document(
  title: "Codon: Architecture and Implementation Plan",
  author: "Codon design",
)

#set page(
  paper: "a4",
  margin: (x: 2.4cm, top: 2.4cm, bottom: 2.4cm),
  numbering: "1 / 1",
  number-align: center,
)

#set text(font: "New Computer Modern", size: 10.5pt, lang: "en")
#set par(justify: true, leading: 0.62em, first-line-indent: 0pt)
#show heading: set block(above: 1.4em, below: 0.8em)
#set heading(numbering: "1.1")

#show raw.where(block: true): block.with(
  fill: luma(245),
  inset: 8pt,
  radius: 3pt,
  width: 100%,
)
#show raw.where(block: false): box.with(
  fill: luma(245),
  inset: (x: 3pt, y: 0pt),
  outset: (y: 3pt),
  radius: 2pt,
)

#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  block(above: 0pt, below: 1em)[#it]
}

// ---------- Title block ----------

#align(center)[
  #v(2.5cm)
  #text(size: 22pt, weight: "bold")[Codon]
  #v(0.3cm)
  #text(size: 14pt)[Architecture and Implementation Plan]
  #v(0.6cm)
  #text(size: 11pt, style: "italic")[Design document v0.1 — May 2026]
  #v(2cm)
  #block(width: 75%)[
    #set text(size: 11pt)
    #set par(justify: true, first-line-indent: 0pt)
    #align(left)[
      _Codon is a keyboard-driven, terminal-first development environment.
      It combines a Zellij-style multiplexer, full Helix editing, and Zed's
      agent and git tooling under a uniform modal UX. A persistent local
      daemon owns all session state and editor logic; a thin window process
      handles only rendering. The same protocol boundaries that decouple the
      window from the daemon let session capabilities — PTYs, filesystem,
      LSP — be served either locally or by a small remote agent over SSH._
    ]
  ]
]

#pagebreak()
#outline(depth: 2, indent: auto)
#pagebreak()

// ============================================================
= Overview
// ============================================================

== What Codon is

Codon is a terminal multiplexer first, an IDE second. The default experience
is a window full of typed panes, most of which are PTYs running shells, with
some panes being graphical surfaces (editor, file browser, agent, git, diff,
image preview) that share the same layout and key model as the terminals.
There is no separate "editor mode" or "IDE mode" — every pane lives under the
same modal shell with a uniform action vocabulary.

Codon is built on three pre-existing codebases, all forked into a single
monorepo and modified as needed:

- *Zed* — for GPUI (the rendering framework), pickers and fuzzy search,
  themes and settings, and the family of "vertical features" Zed has invested
  heavily in: the agent panel, inline assistant, git panel and diff viewer,
  diagnostics aggregation, and the AI-generated commit editor.
- *Helix* — for everything to do with text buffers: rope storage, the
  selection-first editing model, syntax via tree-sitter, LSP client logic.
  Helix is the only buffer-interaction paradigm in Codon.
- *Ghostty* — via `libghostty-vt` (terminal state and VT parsing) and the
  community `gpui-ghostty` integration crate, providing terminal panes that
  share the same rendering substrate as the rest of the UI.

Codon contributes the integration layer above all three: a tiny kernel that
owns sessions and dispatch, a uniform action and keymap system, a modal
"shell" that hosts all pane kinds, two clean protocols for window/daemon and
daemon/capability communication, and a small remote agent (`agentd`) that
makes SSH-multiplexed remote sessions work transparently.

== Design philosophy

Three principles drive the architecture; everything else is a consequence.

#set enum(numbering: "1.")

+ *Keep the kernel small; orchestrate, do not engineer.* The daemon's
  job is to route between user actions, vendored library code, and capability
  providers. It contains no rope, no LSP state, no git repo handle, no
  worktree cache; those live inside the vendored crates. The kernel is
  registries and dispatch.

+ *One transport, two schemas.* The wire format between window and daemon
  and between daemon and capability providers shares plumbing —
  channel multiplexer, framing, sequence numbers, replay — but uses
  separate Cap'n Proto schema sets (one for "view," one for "capabilities").
  Local in-process and remote-over-SSH paths use the same capability schema;
  the kernel doesn't branch on locality.

+ *Terminal first, modal everywhere, uniform across surfaces.* Most panes are
  terminals. All panes — terminal, editor, files, agent, git, diff, image —
  share one modal model (Normal / Insert / Command), one action registry,
  one keymap configuration, one status/command line. Adding a new pane kind
  is a small, well-defined change; it never breaks UX uniformity.

== Goals and non-goals

#table(
  columns: (1fr, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: top,
  table.header(
    [*Goals*], [*Non-goals (for v0)*],
  ),
  [Multiplexer-first UX with typed panes],
  [Multi-window],
  [Full Helix editing for every text buffer],
  [Tabs as a separate concept (use stacked panes)],
  [Persistent daemon-based sessions],
  [WASM plugin runtime],
  [Single-keystroke-roundtrip local UX],
  [Predictive echo / latency hiding (deferred)],
  [Clean SSH remote with small agent binary],
  [Side-by-side display of multiple sessions],
  [Per-session local OR remote capability binding],
  [Mobile, web, or browser deployment],
  [Reuse of Zed's agent, git, diff, diagnostics],
  [In-tree implementation of LSP, ropes, syntax],
  [Reuse of yazi UX idioms (no code reuse)],
  [Stable upstream tracking of vendored projects],
)

The non-goals matter as much as the goals. Each one is a place we are
trading completeness for the ability to ship a coherent v0.

// ============================================================
= Architecture
// ============================================================

== Three processes

Codon runs as up to three cooperating processes:

```
┌──────────────────┐    Unix socket      ┌──────────────────┐
│  codon (window)  │◄───────────────────►│  codon-daemon    │
│  GPUI rendering  │    view protocol    │  kernel + state  │
│  libghostty-vt   │                     │  Helix logic     │
│  Zed view bits   │                     │  Zed agent/git   │
└──────────────────┘                     └────────┬─────────┘
                                                  │
                                                  │ in-process OR
                                                  │ SSH-multiplexed
                                                  │ capability protocol
                                                  ▼
                                     ┌─────────────────────────┐
                                     │ local capabilities      │
                                     │ OR agentd (remote)      │
                                     │ pty · fs · proc · lsp   │
                                     └─────────────────────────┘
```

*The window process* (`codon`) owns the GUI: GPUI compositing, font
rendering, libghostty-vt for terminal panes, syntax highlighting, GPU work.
It has no direct dependencies on Helix, no domain logic, no session state.
When closed, sessions persist; when reopened, it reattaches.

*The daemon process* (`codon-daemon`) owns everything stateful and
authoritative: open sessions, layout trees, Helix `Document`s, LSP clients,
git state, agent conversations. It speaks the view protocol northward to
the window and the capability protocol southward to providers. It has no
GPU or rendering dependencies — by design, so that a future "daemon on the
remote" deployment is feasible without extra work.

*The remote agent* (`agentd`) is a tiny binary that runs on a remote host
over SSH. It implements only the capability protocol; it has no
understanding of sessions, layouts, buffers, or anything UI-shaped. It
exposes PTYs, filesystem operations, processes, file watches, and LSP
relays. Roughly 4 kLOC of focused Rust.

== Layer cake

Bottom-up:

```
┌──────────────────────────────────────────────────────────┐
│ L5  UX SHELL  (window-side)                              │
│     modal panes · status/cmd line · keymap · theme       │
└──────────────────────────────┬───────────────────────────┘
                               │ view protocol (capnp)
┌──────────────────────────────▼───────────────────────────┐
│ L4  KERNEL  (daemon-side; small)                         │
│     sessions · layout · action registry · dispatch       │
│     project context (lazy) · capability multiplexer      │
└──────────────────────────────┬───────────────────────────┘
                               │ capability protocol (capnp)
                ┌──────────────┴──────────────┐
                │                             │
┌───────────────▼──────────────┐   ┌──────────▼─────────────┐
│ L3 LOCAL CAPABILITIES         │   │ L3 REMOTE CAPABILITIES │
│ in-process Rust handlers      │   │ agentd over SSH        │
│ pty · fs · proc · lsp · meta  │   │ same schemas           │
└───────────────────────────────┘   └────────────┬───────────┘
                                                 │
                                    ┌────────────▼──────────┐
                                    │ L2  CHANNELS           │
                                    │ multiplexed framed     │
                                    │ streams w/ seq, credit │
                                    └────────────┬───────────┘
                                                 │
                                    ┌────────────▼──────────┐
                                    │ L1  TRANSPORT          │
                                    │ Unix socket (W↔D)      │
                                    │ SSH ControlMaster (D↔A)│
                                    │ later: mosh-style UDP  │
                                    └────────────────────────┘
```

L1 and L2 are shared infrastructure: the same channel multiplexer
implementation serves both protocol surfaces. L3 is the only layer that
exists redundantly (in-process and over-the-wire), and the wire-side
(`agentd`) is a thin RPC server over the in-process implementation.

L4 is local-only: there is no "kernel on the remote." Operations that
look compound (open a project, stage a hunk, run a build with structured
output) are decomposed by L4 into capability calls on L3. This is what
keeps `agentd` small.

L5 has no business logic; it renders typed render streams from L4 and
sends action invocations and key feeds back.

// ============================================================
= Components
// ============================================================

The repository is a single Cargo workspace with three top-level
directories: `vendor/` for forked code we did not author from scratch,
`crates/` for original Codon code, and `apps/` for binaries.

== Vendored from Zed

All forked into the monorepo. We track upstream selectively rather than
automatically; rebases happen on our schedule.

#table(
  columns: (auto, 1fr, auto),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left, center),
  table.header([*Crate*], [*Use*], [*Modified?*]),
  [`gpui`, `gpui-macros`], [Rendering framework. Window-side. Used essentially as-is.], [no],
  [`picker`, `fuzzy`], [Pickers, nucleo-backed fuzzy search. Used by the command palette and any list UI.], [no],
  [`theme`, `settings`], [Theme system and settings infrastructure. Shared across window and daemon (settings).], [minor],
  [`terminal_view`], [Terminal pane rendering primitives. Underlying terminal core swapped from `alacritty_terminal` to `libghostty-vt` via gpui-ghostty.], [yes],
  [`git` †], [Git status, diff, hunk staging, log, blame. Heavily used by the git pane.], [yes — rewired to `Buffer` trait],
  [`agent` †], [Conversation history, tool use, MCP integration, agent panel UI.], [yes — rewired],
  [`inline_assistant` †], [In-buffer AI editing with diff acceptance.], [yes — rewired],
  [`diff` †], [Diff computation and rendering primitives.], [yes — rewired],
  [`diagnostics` †], [Project-level diagnostic aggregation, panel, hover. Replaces Helix's diagnostics.], [yes — rewired],
  [`commit_editor` †], [AI-assisted commit message generation and commit editor pane.], [yes — rewired],
)

Crates marked † depend on Zed's `Buffer`/`MultiBuffer`/`Project`. They
are forked and re-pointed at Codon's `Buffer` trait (#ref(<sec:buffer-trait>)).
The exact crate names and boundaries in Zed will need to be confirmed
during fork analysis; the table reflects the conceptual carve-up.

== Vendored from Helix

```
vendor/helix/
├── helix-core/        # ropey, selections, transactions, syntax
├── helix-view/        # Document, View — used; Editor.tree ignored
├── helix-lsp/         # LSP client — fed our capability stream as if it were a process
├── helix-loader/      # language config, grammar discovery
└── helix-stdx/        # utility crate Helix depends on
```

We do not vendor `helix-term` (the TUI renderer is replaced by GPUI). The
canonical `Editor` struct in `helix-view` will be used for its document and
view management but its `Tree` (Helix's pane layout) is not used; the
kernel owns layout instead.

== Vendored from gpui-ghostty

The `gpui-ghostty` crate (terminal pane built on GPUI + libghostty-vt) is
forked into `vendor/gpui-ghostty/`. It is window-side only; the daemon
treats PTYs as opaque byte streams.

== New Codon crates

```
crates/
├── codon-buffer/         # Buffer trait + helix_view::Document adapter
├── protocol/             # capnp schemas (view + capability)
├── transport/            # L1: Unix socket, SSH ControlMaster (mosh-UDP later)
├── channels/             # L2: multiplexer, credit-based flow control, replay
├── capabilities-local/   # in-process implementations of L3
├── capabilities-remote/  # auto-generated (or hand-written) client proxies
├── kernel/               # sessions, layout, action registry, dispatch
├── ux-shell/             # modal pane host, status/cmd line, keymap loader
├── panes-terminal/       # daemon-side: drives PTY · window-side: gpui-ghostty wrapper
├── panes-editor/         # daemon-side: drives Helix Document · window-side: pure renderer
├── panes-files/          # yazi-idiom file browser
├── panes-diff/           # uses vendored zed/diff
├── panes-git/            # uses vendored zed/git
├── panes-agent/          # uses vendored zed/agent
├── panes-image/          # local image decode + GPUI render
└── panes-commit/         # uses vendored zed/commit_editor
```

Pane crates are split internally into `daemon` and `window` modules with
disjoint dependencies. The window module never imports Helix; the daemon
module never imports GPUI. This split is enforced by the Cargo workspace.

== Binaries

```
apps/
├── codon/                # the window binary
├── codon-daemon/         # the daemon binary
└── agentd/               # the remote agent binary
```

The window binary is what the user launches. On startup it tries to
connect to `$XDG_RUNTIME_DIR/codon.sock`; if no daemon is listening, it
spawns one and waits ~50 ms for the socket to come up.

// ============================================================
= Protocols
// ============================================================

Codon has two protocol surfaces, both built on the same channel
multiplexer over a swappable transport.

== L1: Transport

Three transports are anticipated; only the first two are needed for v0.

#table(
  columns: (auto, 1fr, auto),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left, center),
  table.header([*Name*], [*Use*], [*Phase*]),
  [`unix`], [Window ↔ daemon, on the same machine.], [v0 (Phase 3)],
  [`ssh`], [Daemon ↔ `agentd`, multiplexed via OpenSSH `ControlMaster`.], [v0 (Phase 6)],
  [`udp`], [Mosh-style: roaming, low-latency, lossy reorder buffer.], [Phase 7+],
)

The transport is a duplex byte stream. It does not see message boundaries.
All transports support graceful disconnect detection and reconnect. The
SSH transport works the way Zed's does: one persistent control master per
remote host, with channels multiplexed inside it.

== L2: Channels

A channel multiplexer carves a single transport into many logical
bidirectional streams. The design borrows from HTTP/2 and gRPC but is
substantially simpler.

A channel has:

- *Channel ID* — `u32`, allocated by the opening side; even IDs from
  client, odd from server (HTTP/2 convention).
- *Schema ID* — `u32`, identifies which capnp schema the channel carries.
  Negotiated via the `meta` capability at connection setup.
- *Sequence numbers* — every message on a channel carries a monotonically
  increasing `seq: u64`. Used for replay on reconnect.
- *Credit-based flow control* — receiver advertises a budget in bytes;
  sender pauses when budget is exhausted. Per-channel, not per-connection.
- *Lifecycle* — `OpenChannel(schemaId, params)` →
  `Opened(channelId)` | `Rejected(error)`. `CloseChannel(channelId, reason)`.

Replay semantics: on reconnect, both sides exchange the highest `seq` they
acknowledge having received per channel. Each replays anything held in its
ring buffer above that threshold. Channels persist across reconnects; the
caller does not need to re-open them.

The wire format for the multiplexer header itself is a fixed 16-byte
binary frame, not capnp:

```
┌────────┬────────┬────────────┬────────────────┐
│  type  │ flags  │ channel_id │      seq       │
│  u8    │  u8    │   u32 LE   │     u64 LE     │
└────────┴────────┴────────────┴────────────────┘
[ payload: capnp message, or control message  ]
```

This keeps the hot path simple. capnp is used only for typed payloads.

== L3: Capability protocol

Six capability domains. Total wire surface is about 150 message types
across all of them. Each domain has a versioned schema; agents declare
which versions they support in the `meta.Hello` exchange.

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left),
  table.header([*Capability*], [*Surface*]),
  [`meta`], [`hello`, `ping`, `goodbye`, capability discovery, version negotiation, time sync.],
  [`pty`], [`spawn`, `feed`, `resize`, `close`, plus per-handle output stream.],
  [`fs.io`], [`read`, `write`, `stat`, `list`, `mkdir`, `rename`, `delete`. Synchronous request/response.],
  [`fs.watch`], [`subscribe(path, recursive)` → handle + per-handle event stream. `unsubscribe`.],
  [`proc`], [`spawn(cmd, args, env, cwd)`, plus per-job stdout/stderr streams and exit-code response.],
  [`lsp`], [`spawn(language)` → handle + bidirectional JSON-RPC byte stream. `kill`. The agent does no JSON parsing — it relays bytes.],
)

What is conspicuously not in the capability protocol: anything compound.
There is no `git.status` or `workspace.open` or `buffer.edit`. Compound
operations are composed in the kernel from these primitives. This is the
single decision that keeps `agentd` small.

A capnp sketch for `pty`:

```capnp
# pty.capnp
@0xab12cd34;

struct Size { rows @0 :UInt16; cols @1 :UInt16; }
struct EnvVar { name @0 :Text; value @1 :Text; }

struct PtyHandle { id @0 :UInt64; }

# Request/response on the control channel
struct PtySpawnRequest {
  cmd @0 :List(Text);
  env @1 :List(EnvVar);
  cwd @2 :Data;          # raw bytes; no encoding assumed
  size @3 :Size;
}
struct PtySpawnResponse {
  union {
    handle @0 :PtyHandle;
    error  @1 :Error;
  }
}

struct PtyFeed   { handle @0 :PtyHandle; bytes @1 :Data; }
struct PtyResize { handle @0 :PtyHandle; size  @1 :Size; }
struct PtyClose  { handle @0 :PtyHandle; }

# Per-handle output stream channel
struct PtyOutput {
  handle @0 :PtyHandle;
  bytes  @1 :Data;
}
struct PtyExit {
  handle @0 :PtyHandle;
  code   @1 :Int32;
}
```

Schema versioning is per-domain. `pty.v1` and `pty.v2` may coexist on a
single agent; the daemon picks the highest mutually-supported version.
Within a major version, additions are backwards-compatible (capnp default
behavior).

== Capability protocol: in-process and over-the-wire are identical

The kernel makes capability calls through a single `Capabilities` trait.
Both `capabilities-local` (which spawns local PTYs, opens local files,
talks to local LSP servers) and `capabilities-remote` (which serializes
capnp messages onto an SSH-multiplexed channel set) implement this trait.

```rust
// crates/kernel/src/capabilities.rs
trait Capabilities {
    fn pty(&self) -> &dyn PtyCapability;
    fn fs_io(&self) -> &dyn FsIoCapability;
    fn fs_watch(&self) -> &dyn FsWatchCapability;
    fn proc(&self) -> &dyn ProcCapability;
    fn lsp(&self) -> &dyn LspCapability;
    fn meta(&self) -> &dyn MetaCapability;
}
```

A `Session` holds one `Box<dyn Capabilities>`. Local sessions hold a
local-impl; remote sessions hold a remote-impl. Pane code never branches
on locality.

== L4: View protocol

The view protocol is everything that flows between the window and the
daemon. It is conceptually distinct from capabilities and uses different
schemas, but rides the same channel multiplexer.

*Window → Daemon:*

```capnp
# view.capnp (excerpt)
struct DispatchAction {
  name @0 :Text;            # "editor.move_word_forward", "pane.split", ...
  args @1 :ActionArgs;      # capnp union, see actions.capnp
  context @2 :ActionContext; # focused session, focused pane, etc.
}

struct FeedKeys {
  paneId @0 :PaneId;
  keys @1 :Data;            # raw bytes (UTF-8 + escape sequences)
}

struct SubscribePane   { paneId @0 :PaneId; }
struct UnsubscribePane { paneId @0 :PaneId; }
struct ResizePane      { paneId @0 :PaneId; rows @1 :UInt16; cols @2 :UInt16; }
struct NotifyFocus     { sessionId @0 :SessionId; paneId @1 :PaneId; }
```

*Daemon → Window:* one `SessionTopology` stream and one render stream per
subscribed pane. The render stream's schema depends on the pane kind:

```capnp
# view-render.capnp (excerpt)
struct PaneRender {
  paneId @0 :PaneId;
  union {
    terminal @1 :TerminalRender;
    editor   @2 :EditorRender;
    files    @3 :FilesRender;
    diff     @4 :DiffRender;
    image    @5 :ImageRender;
    git      @6 :GitRender;
    agent    @7 :AgentRender;
    commit   @8 :CommitRender;
  }
}

struct TerminalRender {
  bytes @0 :Data;           # raw VT, parsed by libghostty-vt in the window
}

struct EditorRender {
  # rope deltas, not full content
  textDelta    @0 :RopeDelta;
  selection    @1 :Selection;
  diagnostics  @2 :List(Diagnostic);
  highlights   @3 :HighlightDelta;
  scroll       @4 :ScrollPosition;
  viewportLine @5 :UInt32;
}
```

The asymmetry is deliberate. The window never holds authoritative state;
it derives presentation from typed deltas. For terminal panes the
"derivation" is libghostty-vt's VT parsing; for editor panes it's GPUI's
text layout fed by rope deltas; for image panes it's a one-shot decode.

Subscriptions are explicit and bounded: the window only subscribes to
panes that are visible (or about to be). Closing a pane unsubscribes.
This is what keeps the daemon's outbound bandwidth tractable when, e.g.,
multiple terminal panes are running noisy commands.

== Observability and replay

Every channel — view or capability — carries `seq` numbers. Either side
may request `RequestReplayFrom(channelId, seq)` after reconnect. Each
side maintains a ring buffer (default: 1 MiB per output channel) of
recent messages.

A `--trace` flag on `codon-daemon` writes every incoming and outgoing
multiplexer frame as JSON-pretty-printed capnp messages to a log file.
This is the canonical debugging surface; reading the trace tells you
exactly what the protocols look like in motion.

// ============================================================
= The Kernel
// ============================================================

The kernel is the daemon's coordination layer. It is deliberately small.

== Scope

```
crates/kernel/
├── sessions.rs      # SessionId → Session
├── project.rs       # lazy ProjectContext: root, git, lsp, diagnostics
├── layout.rs        # LayoutNode tree per session
├── panes.rs         # PaneId → typed PaneKind handle
├── actions.rs       # action registry
├── dispatch.rs      # unified entry: receives DispatchAction, runs handler
├── persistence.rs   # save/load sessions to $XDG_STATE_HOME/codon/
└── capabilities.rs  # Capabilities trait, set on each Session
```

What the kernel deliberately does not contain:

- File contents — fetched on demand via `fs.io`, held by Helix `Document`s when open.
- Worktree caches — caching, if needed, is per-capability and transparent.
- LSP client state — owned by `helix-lsp` instances inside the daemon (not the kernel crate).
- Git repo state — owned by Zed's vendored git crate, behind the `Buffer` trait.
- PTY state — owned by `panes-terminal` daemon module, fed by the `pty` capability.
- Theme/keymap — owned by `ux-shell` / window side via `settings` plumbing.

In short, the kernel is a registry-and-dispatch layer. It has handles, not
data.

== Sessions and lazy promotion

```rust
struct Session {
    id: SessionId,
    name: String,
    cwd: PathBuf,
    capabilities: Box<dyn Capabilities>,  // Local or Remote
    layout: LayoutNode,
    panes: HashMap<PaneId, PaneKind>,
    project: ProjectContext,              // built lazily
    created: Instant,
    last_attached: Option<Instant>,
}

#[derive(Default)]
struct ProjectContext {
    root: OnceCell<Option<PathBuf>>,
    git: OnceCell<Option<GitRepo>>,
    lsp_clients: HashMap<LanguageId, LspClientHandle>,
    diagnostics: DiagnosticStore,         // shared with Zed's diagnostics crate
}
```

A new session is cheap: a name, a cwd, a capability set, an empty layout
with one terminal pane. Project context starts empty.

Project context accretes lazily, triggered by the first pane that needs
it:

- *First editor pane opens.* The kernel walks up from `cwd` looking for
  language manifests (`Cargo.toml`, `package.json`, etc.) and `.git`. If
  found, `project.root` is set. The pane's language is detected; an LSP
  client for that language is spawned via the `lsp` capability and
  registered in `project.lsp_clients`.
- *First git pane opens.* `project.git` is set if the cwd is inside a
  git work tree; otherwise the pane shows a "no repo" view.
- *First diagnostic arrives.* From any LSP client, gets stored in
  `project.diagnostics`, which is consumed by the diagnostics panel,
  the editor's gutter, and the agent.

A session never un-promotes. The trade-off: starting a fresh session for
work is essentially free; using an existing session inherits all its
warmed-up state.

== Layout

Three node types, no tabs:

```rust
enum LayoutNode {
    Split { dir: Direction, ratio: f32, a: Box<LayoutNode>, b: Box<LayoutNode> },
    Stack { panes: Vec<PaneId>, visible: usize },
    Leaf(PaneId),
}

enum Direction { Horizontal, Vertical }
```

A session has exactly one `LayoutNode` as root. Stacks substitute for
tabs: when you want N variants in the same slot (multiple shells, an
editor and a files pane occupying the same area), stack them and cycle
visibility with `pane.stack.cycle`.

== Action registry

```rust
type ActionFn = fn(&mut Kernel, &ActionContext, ActionArgs) -> Result<()>;

struct Registry {
    actions: HashMap<&'static str, ActionFn>,
}

struct ActionContext {
    session: SessionId,
    pane: Option<PaneId>,
    mode: Mode,
}
```

Action names are flat strings in dotted namespaces. Examples:

```
editor.move_word_forward       editor.extend_word_forward
editor.search                  editor.write
pane.split                     pane.focus.left
pane.stack.add                 pane.stack.cycle
pane.close
session.new                    session.switch
session.list
file.open                      file.delete
terminal.new                   terminal.scroll.up
git.diff.current               git.stage.hunk
agent.task.run                 agent.message.send
workspace.command_palette
```

`ActionArgs` is a capnp union; specific actions take typed args. The
keymap binds keys to `(name, args)` pairs:

```toml
[bindings.global]
"alt-h"        = "pane.focus.left"
"alt-l"        = "pane.focus.right"
"alt-s"        = "session.switch"
"ctrl-space"   = "workspace.command_palette"
"alt-|"        = { action = "pane.split", args = { dir = "right", kind = "terminal" } }
"alt-shift-|"  = { action = "pane.split", args = { dir = "right", kind = "files" } }
"alt-shift-e"  = { action = "pane.split", args = { dir = "right", kind = "editor" } }

[bindings.terminal.normal]
"|"            = { action = "pane.split", args = { dir = "right", kind = "terminal" } }
"i"            = "pane.mode.insert"

[bindings.editor.normal]
# Helix's full normal-mode keymap, imported verbatim
# (these are populated from a generated table)
```

The default keymap for editor Normal mode is generated from Helix's own
`languages.toml` and command set; we then overlay user overrides on top.

== Dispatch

The dispatcher is the single entry point for action handling on the
daemon side:

```rust
impl Kernel {
    fn dispatch(&mut self, ctx: ActionContext, name: &str, args: ActionArgs)
        -> Result<()>
    {
        let action = self.registry.actions
            .get(name)
            .ok_or(Error::UnknownAction)?;
        action(self, &ctx, args)
    }
}
```

`DispatchAction` messages from the window land here. `feedKeys` messages
go directly to the focused pane's daemon-side handler (e.g. terminal pane
forwards to `pty.feed`; editor pane runs Helix's input handler).

// ============================================================
= Buffer trait and pane integration
// ============================================================

<sec:buffer-trait>

== The integration problem

Codon uses Helix's editing model for all text buffers. But it also reuses
Zed's git, agent, inline-assistant, diff, diagnostics, and commit-editor
crates — all of which were written against Zed's own `Buffer` /
`MultiBuffer` / `Project` types. These two stacks are not API-compatible.

We have two options:

#table(
  columns: (1fr, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: top,
  table.header([*A. Wrapper*], [*B. Trait (proposed)*]),
  [Implement Zed's full `Buffer` API in terms of Helix's `Document`. Zed crates use it unchanged.],
  [Define a small `codon_buffer::Buffer` trait. Fork Zed crates, replace their `&Buffer` with `&dyn codon_buffer::Buffer`. Helix `Document` implements the trait.],
  [Pro: Zed's vendored crates need no modification.],
  [Pro: explicit surface, semantic gaps caught at compile time.],
  [Con: semantic gaps leak through; Zed's API is large; mismatches surface at runtime.],
  [Con: need to refactor every Zed crate that touches buffers.],
)

The trait approach is preferred but the decision is provisional pending a
detailed analysis of how deeply Zed's buffer dependencies penetrate the
crates we want to use. That analysis is part of Phase 2 (#ref(<sec:phase-2>)).

== Trait sketch (provisional)

```rust
// crates/codon-buffer/src/lib.rs
pub trait Buffer: Send + Sync {
    fn id(&self) -> BufferId;
    fn rope(&self) -> &ropey::Rope;
    fn apply_edits(&mut self, edits: &[Edit]) -> Transaction;
    fn selection(&self) -> &helix_core::Selection;
    fn diagnostics(&self) -> &[Diagnostic];
    fn language(&self) -> Option<&Language>;
    fn syntax_tree(&self) -> Option<&tree_sitter::Tree>;
    fn observe(&mut self, sink: BufferEventSink);
    fn position_to_offset(&self, pos: Position) -> usize;
    fn offset_to_position(&self, off: usize) -> Position;
    // ~20 methods total
}
```

The shape mirrors Helix's `Document`. Zed's higher-level features
(diff hunks, inline assistant, commit editor) consume only this trait;
they never see the concrete type.

== Pane crate structure

Every pane crate is split into two modules with disjoint dependencies:

```
crates/panes-editor/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── daemon.rs       # uses helix-core, helix-view, helix-lsp
    └── window.rs       # uses gpui, theme, fuzzy
```

The Cargo workspace is configured so that the `daemon.rs` and `window.rs`
modules compile to separate library targets with separate dependency
sets. The window-side never transitively pulls in `helix-*`; the
daemon-side never pulls in `gpui`. This is enforced by build configuration,
not just convention.

The daemon side of `panes-editor` runs in the daemon process; it owns the
`helix_view::Document`, executes Helix commands when actions arrive, and
emits `EditorRender` deltas. The window side runs in the window process;
it consumes the deltas and renders text via GPUI (using vendored Zed text
rendering primitives, not Helix's TUI rendering).

For pane kinds whose daemon side uses Zed crates (`git`, `agent`,
`inline_assistant`, `commit`), the daemon module pulls in the
relevant vendored Zed crate and rewires its buffer dependencies to
`codon_buffer::Buffer`. The window module has the same shape regardless
of which engine drives it.

// ============================================================
= UX shell
// ============================================================

The UX shell is the window-side framework that hosts panes uniformly. It
is small in code but defines the invariants that make Codon feel coherent.

== Modal model

Three modes per pane: *Normal*, *Insert*, *Command*. Mode is per-pane,
not per-window — different panes can be in different modes simultaneously.

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left),
  table.header([*Mode*], [*Behavior*]),
  [Normal], [Keys execute named actions according to the keymap. Default for editor, files, git, agent, diff, image. Pane-kind-specific keymap section applies.],
  [Insert], [Keys reach the pane's underlying widget. In a terminal, that means raw bytes to the PTY. In an editor, typed input into the buffer (with Helix's insert-mode commands available). In files, fuzzy-filter input. Default for terminal panes only.],
  [Command], [`:` opens a command line at the bottom; tab completes against the action registry. Action invoked on Enter. Same in every pane.],
)

== Keymap layering

The keymap config has four scopes, applied in order:

#set enum(numbering: "1.")

+ *Global* — works in every mode, every pane. Used for pane navigation,
  session switching, command palette open, and other cross-cutting
  operations. These bindings always win.
+ *Per-pane-kind, per-mode* — `[bindings.editor.normal]`,
  `[bindings.terminal.insert]`, etc. The pane's kind and current mode
  select the section.
+ *Per-mode* — `[bindings.normal]`. Applies in any pane in that mode if
  no per-pane-kind binding shadows it.
+ *Default* — Helix's keymap for editor modes, hard-coded sensible
  defaults for everything else. User overrides displace the default.

The window resolves keys against this stack and either:

- Emits a `DispatchAction` to the daemon for resolved actions, or
- Emits a `FeedKeys` to the daemon if no binding matched and the pane is
  in a mode that accepts raw input (Insert, primarily).

The keymap is hot-reloadable. Changes to the config file invalidate the
window's cached resolver.

== Status and command line

A single bottom-line widget renders, depending on mode:

- *Normal/Insert:* status — focused pane info, mode indicator, session
  name, capability target (e.g. `local` or `remote: lab-machine`),
  optional widgets (git branch, diagnostic counts, currently playing
  music for the eccentric).
- *Command:* a `:` prompt with a fuzzy-completing input.

The status content is sourced from a `StatusLine` view-protocol stream,
formatted by the UX shell. Format strings live in user config, similar
to zjstatus.

== Pane host

The pane host is the GPUI component that owns a `LayoutNode` and renders
its children. It is recursive:

- A `Split` node renders two child panes side-by-side or stacked, with a
  draggable (keyboard-resizable) separator.
- A `Stack` node renders only the visible pane, with an indicator
  (similar to Zellij's stacked-pane indicator) showing position in the
  stack.
- A `Leaf` node renders a single pane via the appropriate window-side
  pane component.

The host subscribes to the daemon for the focused session's topology.
When the daemon sends a `SessionTopology` update, the host diffs against
its current tree and animates the change.

// ============================================================
= Sessions and capability binding
// ============================================================

== Per-session capability sets

The kernel does not have a global capability set. Each `Session` carries
its own:

```rust
enum CapabilitySet {
    Local,
    Remote { host: SshHost, agent: AgentHandle },
}
```

A new session is created with one or the other:

```
session.new                              # local, cwd = $HOME
session.new { cwd = "~/projects/foo" }   # local, specific cwd
session.new { remote = "lab-machine" }   # remote, default home on lab-machine
session.new { remote = "lab-machine", cwd = "~/research" }
```

Sessions of both kinds coexist in the same `codon-daemon`. Switching
sessions is purely a UI operation; it does not connect or disconnect
anything. Capabilities are connected when a session is created and
released when it is closed (or after a configurable idle timeout for
remote sessions).

Within a session, all panes share that session's capability set. You
cannot mix local and remote panes in one session — that would require the
project context to span hosts, which doesn't make sense.

== Persistence and the daemon lifecycle

The daemon runs as a long-lived per-user process. State is persisted to
disk in two ways:

#set enum(numbering: "1.")

+ *In-memory authoritative state* — all sessions, layouts, open buffers,
  capability connections. Survives window closes.
+ *Periodic snapshots to disk* — written to
  `$XDG_STATE_HOME/codon/sessions/<session-id>/` every 30 seconds and
  on graceful shutdown. Used to rehydrate after daemon restart (planned
  upgrades, crashes).

What gets persisted per session:

- Session metadata (name, cwd, capability target, creation time)
- Layout tree
- Per-pane state:
  - Terminal: the PTY does not survive daemon restart; on rehydrate,
    the pane is replaced with a "session restored — press Enter to
    respawn" placeholder showing the last 1 MiB of scrollback.
  - Editor: open file paths and view state; rope content is re-read.
    Unsaved changes are persisted as a swap file.
  - Files / git / diff / image: just enough to recreate the pane.
  - Agent: full conversation history (this is large — Zed's agent
    crate already persists this).

The first time the window connects after a daemon restart, the user sees
their session list as before. Terminals show placeholders; editors are
rehydrated; agent conversations resume.

Disconnect from a remote session:

- *Window closes:* daemon keeps the session, capability connection stays
  open until idle timeout.
- *Network drops:* daemon detects loss, marks the session as
  "disconnected," holds it for `remote.reconnect_timeout` (default 5
  minutes), then closes the SSH connection but keeps the local session
  state. On user attempt to use the session, daemon reconnects SSH and
  replays from sequence numbers.
- *Daemon restart while disconnected:* on rehydrate, remote sessions
  start in a "disconnected" state; reconnect on first use.

== Session list and switching

The session list is a UI concept rendered as a picker (using vendored
Zed `picker`/`fuzzy`). It shows session name, cwd, capability target,
last-attached time, and a preview snapshot.

`session.switch` opens the picker; `session.new` opens a "create session"
flow with cwd and remote-host fields. There is no "current session"
hierarchy beyond what the user sees in the picker.

// ============================================================
= Implementation plan
// ============================================================

The plan is sequential phases, each ending in a usable artifact. No phase
leaves the codebase in an unshipping state.

== Phase 0 — Walking skeleton

*Goal:* a window that opens, contains one terminal pane (gpui-ghostty)
and one editor pane (Helix `Document` rendered through GPUI).

Build steps:

- Set up the monorepo. Vendor Zed (`gpui`, `gpui-macros`, `picker`,
  `fuzzy`, `theme`, `settings`, `terminal_view`). Vendor Helix
  (`helix-core`, `helix-view`, `helix-lsp`, `helix-loader`,
  `helix-stdx`). Vendor `gpui-ghostty`.
- Replace `terminal_view`'s `alacritty_terminal` dependency with
  `libghostty-vt` via gpui-ghostty.
- Build a minimal window that opens a single GPUI surface containing
  one terminal pane and one editor pane, side-by-side.
- Hardcode the editor pane to open one file (`/etc/hostname` or
  similar). Render Helix's `Document` content through a small custom
  GPUI element that produces text from the rope. No Helix commands
  yet — read-only.
- Hardcode the terminal pane to spawn `$SHELL`.

*Deliverable:* `cargo run` opens a window. You see a shell on the left,
a file's contents on the right. Nothing is configurable yet.

This phase establishes that GPUI, libghostty-vt, and Helix can coexist
in one binary.

== Phase 1 — Action layer and modal shell

*Goal:* Helix's full editing model works in the editor pane; pane
focus, splits, and the command palette work uniformly across pane kinds;
the keymap is configurable.

Build steps:

- Build `crates/ux-shell` with the modal pane host (Normal / Insert /
  Command), the status/command line, and the keymap loader.
- Build `crates/kernel` minimally: action registry only. No sessions
  yet, no protocol; the kernel runs in-process inside the window.
- Wire Helix's command set into the registry under `editor.*`. Generate
  the registration from Helix's existing command tables.
- Add the cross-cutting actions: `pane.focus.*`, `pane.split.*`,
  `pane.close`, `workspace.command_palette`, `editor.write` (calls
  `fs.io.write` directly).
- Implement the command palette using vendored `picker`/`fuzzy`.
- Implement a TOML keymap loader; ship a default keymap that imports
  Helix's normal-mode bindings verbatim.

*Deliverable:* a one-window Codon with a working Helix editor (full
keymap, modal model, save), a working terminal, and a working command
palette. Single binary, no daemon yet, no protocol yet, no sessions.

== Phase 2 — Kernel split and Buffer trait

<sec:phase-2>

*Goal:* the kernel is a separate `codon-daemon` process; the window
talks to it over a Unix socket; the `Buffer` trait is in place; Zed's
git crate is forked, rewired, and integrated as the first
buffer-consuming feature.

Build steps:

- Spin out `codon-daemon` as a separate binary. Implement the local
  view-protocol transport (Unix socket) without channel multiplexing
  yet — just one persistent stream with simple framing. This is
  temporary; Phase 3 replaces it.
- Define `crates/codon-buffer` with the `Buffer` trait. Implement
  it for `helix_view::Document`. Document the trait surface.
- *Analyze fork impact:* survey Zed's git crate for buffer
  dependencies. Decide trait-vs-wrapper based on what we find. Update
  this design doc with the decision before proceeding.
- Fork Zed's git crate into `vendor/zed/git/`. Rewire its buffer
  dependencies to `codon-buffer::Buffer`. Wire it into Codon as the
  `panes-git` daemon module.
- Add a `panes-git` window module that renders git status, diff,
  log, and hunk staging using GPUI.

*Deliverable:* Codon is now a window-and-daemon pair. You can open a
git pane, see status, view diffs, and stage hunks for any file open in
an editor pane. The `Buffer` trait abstraction is proven.

Risks: the git rewire reveals that Zed's git crate has buffer
dependencies deeper than the trait can express. Mitigation: the
analysis step is a deliverable; if the trait can't bridge, we adopt the
wrapper approach and revise the plan.

== Phase 3 — Protocol formalization

*Goal:* both protocols (view and capability) use capnp on a shared
channel multiplexer. Local capabilities go through the same wire format
as remote ones will. No remote yet.

Build steps:

- Define capnp schemas for both protocols. Build the codegen pipeline.
- Implement `crates/channels`: the multiplexer, sequence numbers,
  credit-based flow control, replay buffers.
- Implement `crates/transport`: the Unix-socket transport. Replace the
  Phase-2 ad-hoc framing.
- Implement `crates/capabilities-local`: local capability handlers
  (PTY, fs, proc, lsp) that integrate with the channel multiplexer.
- Refactor the kernel's existing capability calls to go through the
  multiplexer. Performance penalty: irrelevant; correctness gain:
  large.
- Add the `--trace` flag and the JSON-pretty-printed log output.

*Deliverable:* protocols are formalized; observability tooling works;
the architecture is correct. User-visible: nothing changes. This phase
exists to make Phase 6 a small effort instead of a large one.

== Phase 4 — Native UX coverage

*Goal:* the file browser, diff viewer, and image preview panes work.
Diagnostics are wired in. Codon is a coherent local IDE.

Build steps:

- Build `panes-files`: yazi-idiom three-column file browser. Uses
  `fs.io` and `fs.watch`. Modal: Normal mode for navigation, Insert
  for fuzzy filter, Command for `:`-actions (`:rename`, `:delete`,
  `:trash`, `:archive`).
- Build `panes-diff`: integrates Zed's vendored diff crate. Renders
  diff hunks with syntax highlighting from Helix.
- Build `panes-image`: local image decode with the `image` crate;
  rendered through GPUI. The daemon serves bytes via `fs.io.read`;
  decode and display happen in the window.
- Fork and rewire Zed's `diagnostics` crate. Replace Helix's
  diagnostics throughout. Editor pane gutters and the diagnostics
  panel both consume the same store.
- Add a status-line widget for diagnostic summary.

*Deliverable:* end-to-end coherent IDE. File browse, edit, diff, view,
session-aware. Single user-visible binary.

== Phase 5 — Git, Agent, Inline Assistant, Commit Editor

*Goal:* AI-augmented coding works. Conversations persist. Inline edits
roundtrip through the daemon. Git commit messages can be auto-generated.

Build steps:

- Fork Zed's `agent`, `inline_assistant`, and `commit_editor` crates;
  rewire to `codon-buffer::Buffer`.
- Build `panes-agent` and `panes-commit`. Rewire MCP integration if
  needed. Verify conversation persistence works through the daemon's
  on-disk state.
- Wire the inline assistant to the editor pane: keybinding triggers a
  ranged AI edit; resulting diff is shown inline; user accepts/rejects
  per hunk.
- Wire commit-message auto-generation in the commit editor pane.
- Add agent-aware status-line widgets (running task indicator,
  notification on completion).

*Deliverable:* Codon is feature-competitive with Zed for local
agentic coding, but with Helix as the editor and a multiplexer-first
UX.

== Phase 6 — Remote

*Goal:* a session can be remote. SSH transport works. `agentd` is
deployed. Reconnect-and-replay handles flaky links.

Build steps:

- Build `apps/agentd`: the remote binary. Implements the L2 channel
  multiplexer and the L3 capability handlers. ~4 kLOC target.
- Build `crates/capabilities-remote`: client-side proxies that
  serialize capability calls onto channels.
- Add the SSH transport to `crates/transport`. Use OpenSSH
  `ControlMaster` for connection persistence and multiplexing
  (Zed's pattern).
- Implement automatic `agentd` deployment to the remote: detect arch,
  upload the binary, exec via SSH, handshake.
- Add session-creation flow for remote sessions:
  `session.new { remote = "host" }`.
- Implement reconnect-and-replay end to end. Test with simulated
  network drops.

*Deliverable:* remote sessions work. The same key model, the same UI,
the same agent-and-git tooling, transparently against a remote box.

== Phase 7 — Mosh-equivalent (deferred)

*Goal:* roaming, low-latency remote operation over UDP. Predictive
local echo for terminal panes.

Out of scope for v0. Listed for completeness.

== Summary of phases

#table(
  columns: (auto, 1fr, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left, left),
  table.header([*Phase*], [*Outcome*], [*Shippable as*]),
  [0], [GPUI + libghostty-vt + Helix coexist in one window.], [internal demo],
  [1], [Helix editing + uniform modal shell + command palette.], [single-binary "graphical Helix"],
  [2], [Daemon split, `Buffer` trait, git pane.], [Codon v0.1],
  [3], [capnp-on-channel-mux protocols, observability.], [Codon v0.2],
  [4], [Files, diff, image, diagnostics.], [Codon v0.3],
  [5], [Agent, inline assistant, commit editor.], [Codon v0.4],
  [6], [Remote sessions over SSH.], [Codon v0.5],
  [7], [Mosh-equivalent.], [Codon v0.6+],
)

// ============================================================
= Open questions and decisions deferred
// ============================================================

== Confirmed decisions

For reference, these are locked in for v0 and not up for re-litigation:

- Project name: *Codon*.
- Wire format: *Cap'n Proto*.
- Editing model: *Helix everywhere*.
- Protocol shape: *two schemas (view, capability), one channel
  multiplexer, one transport abstraction*.
- Process model: *window + codon-daemon + agentd (remote only)*.
- Layout: *Split / Stack / Leaf, no tabs*.
- Default pane kind: *terminal*.
- Display model: *one session shown at a time, no side-by-side*.
- Window helix dependency: *zero — window only renders typed schemas*.
- Multi-window: *deferred*.
- WASM plugins: *deferred*.
- Predictive echo: *deferred*.
- yazi reuse: *patterns only, no code*.
- Vendoring strategy: *fork everything into the monorepo*.
- Diagnostics: *Zed's, not Helix's*.

== Provisional / to be analyzed

- *Buffer trait vs wrapper.* Phase 2 includes a survey of Zed's
  buffer-dependent crates. The decision is provisional pending that
  analysis. If trait can't express what's needed, we revisit.
- *Helix gutter rendering.* Helix's gutter (line numbers,
  diagnostic markers, breakpoints, change indicators) is currently
  TUI-rendered. We need to either (a) reimplement the gutter as a GPUI
  component reading from Codon's diagnostic store and Helix's
  `Document` state, or (b) delegate to vendored Zed gutter
  rendering. Likely (a); decide in Phase 1.
- *vim-style shell hook for editor invocation.* Detecting `vim foo.rs`
  in a terminal and routing to an editor pane — opt-in, planned but
  not blocking; design in Phase 4.
- *Settings storage location and format.* Zed uses a JSON config; Helix
  uses TOML. Codon is TOML-first (matches keymap config). Where does
  the settings file live; how do per-project settings overlay user
  settings; how do remote sessions resolve settings.
- *Theme system.* Zed's theme infrastructure works; Helix has its own
  themes. We use Zed's; Helix's TOML themes need to be importable.

== Out of scope, not deferred

These are decisions we are not making:

- Cross-platform support for the daemon. Linux first; macOS likely
  works for free; Windows is not a goal and may never be one.
- A web client. Zed has one; Codon does not.
- Collaboration (multi-user editing). Zed has it; Codon does not aim
  to. The architecture would not preclude it but we do not optimize
  for it.
- Browser-based remote terminal access (in the manner of `ttyd` or
  Zellij's web client). Possible later via the existing transport
  abstraction; not in scope.

// ============================================================
= Appendices
// ============================================================

== Schema sketches

This section collects the most salient capnp schema fragments from
across the document for reference.

=== Channel envelope (binary, not capnp)

```
┌────────┬────────┬────────────┬────────────────┐
│  type  │ flags  │ channel_id │      seq       │
│  u8    │  u8    │   u32 LE   │     u64 LE     │
└────────┴────────┴────────────┴────────────────┘
[ payload — capnp framed message or control ]
```

`type` discriminates: control frame (open, close, replay-request, ack)
vs. payload frame.

=== meta.capnp

```capnp
@0x...;

struct Hello {
  protocolVersion @0 :UInt32;
  agentVersion @1 :Text;
  capabilities @2 :List(CapabilityVersion);
  hostInfo @3 :HostInfo;
}

struct CapabilityVersion {
  name @0 :Text;          # "pty", "fs.io", ...
  versions @1 :List(UInt32);
}

struct HostInfo {
  os @0 :Text;
  arch @1 :Text;
  hostname @2 :Text;
  uptime @3 :UInt64;
}

struct Ping { nonce @0 :UInt64; sentAt @1 :UInt64; }
struct Pong { nonce @0 :UInt64; sentAt @1 :UInt64; recvAt @2 :UInt64; }
```

=== fs.capnp (excerpt)

```capnp
@0x...;

struct ReadRequest {
  path @0 :Data;
  offset @1 :UInt64;
  length @2 :UInt64;       # 0 means "to end"
}
struct ReadResponse {
  union {
    bytes @0 :Data;
    error @1 :Error;
  }
}

struct WriteRequest {
  path @0 :Data;
  offset @1 :UInt64;
  bytes @2 :Data;
  truncate @3 :Bool;
  createMode @4 :UInt32;
}

struct ListRequest {
  path @0 :Data;
  recursive @1 :Bool;
  followSymlinks @2 :Bool;
  globPattern @3 :Text;    # optional
}
struct ListResponse {
  union {
    entries @0 :List(DirEntry);
    error @1 :Error;
  }
}

struct DirEntry {
  name @0 :Data;
  kind @1 :EntryKind;
  size @2 :UInt64;
  mtime @3 :UInt64;
  mode @4 :UInt32;
}
enum EntryKind { file @0; dir @1; symlink @2; other @3; }

struct WatchSubscribe {
  path @0 :Data;
  recursive @1 :Bool;
}
# Subscribe response opens an output stream channel.

struct FsEvent {
  path @0 :Data;
  kind @1 :EventKind;
}
enum EventKind {
  created @0;
  modified @1;
  deleted @2;
  renamed @3;
  metadataChanged @4;
}
```

=== view.capnp (excerpt)

```capnp
@0x...;

# ---- window → daemon ----

struct DispatchAction {
  name @0 :Text;
  args @1 :ActionArgs;
  context @2 :ActionContext;
}

struct ActionArgs {
  union {
    none @0 :Void;
    paneSplit @1 :PaneSplitArgs;
    paneFocus @2 :PaneFocusArgs;
    sessionNew @3 :SessionNewArgs;
    # ... many more
  }
}

struct PaneSplitArgs {
  dir @0 :Direction;
  kind @1 :PaneKind;
  args @2 :PaneOpenArgs;   # kind-specific
}

enum Direction { right @0; down @1; left @2; up @3; }
enum PaneKind {
  terminal @0; editor @1; files @2; diff @3;
  image @4; git @5; agent @6; commit @7;
}

# ---- daemon → window ----

struct SessionTopology {
  sessions @0 :List(SessionEntry);
  focused @1 :SessionId;
  layout @2 :LayoutNode;       # for the focused session only
  paneModes @3 :List(PaneMode); # for all panes in the focused session
}

struct LayoutNode {
  union {
    leaf @0 :PaneId;
    split @1 :SplitNode;
    stack @2 :StackNode;
  }
}

struct SplitNode {
  dir @0 :Direction;
  ratio @1 :Float32;
  a @2 :LayoutNode;
  b @3 :LayoutNode;
}

struct StackNode {
  panes @0 :List(PaneId);
  visible @1 :UInt32;
}
```

=== Per-pane render schemas (excerpt)

```capnp
struct PaneRender {
  paneId @0 :PaneId;
  generation @1 :UInt64;        # for debugging out-of-order arrivals
  union {
    terminal @2 :TerminalRender;
    editor @3 :EditorRender;
    files @4 :FilesRender;
    # ... others
  }
}

struct TerminalRender {
  bytes @0 :Data;               # raw VT, libghostty-vt parses
}

struct EditorRender {
  union {
    full @0 :EditorFullState;   # initial subscribe
    delta @1 :EditorDelta;      # incremental
  }
}

struct EditorFullState {
  text @0 :Text;
  selection @1 :Selection;
  diagnostics @2 :List(Diagnostic);
  language @3 :Text;
  scroll @4 :ScrollPosition;
}

struct EditorDelta {
  textChanges @0 :List(TextChange);
  selection @1 :Selection;
  diagnosticsDelta @2 :DiagnosticsDelta;
  scroll @3 :ScrollPosition;
}

struct TextChange {
  range @0 :Range;
  newText @1 :Text;
}
```

== Glossary

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 6pt,
  align: (left, left),
  [*Action*], [A named operation in the kernel's registry; invoked by keymap or command palette.],
  [*Capability*], [A typed RPC surface (PTY, fs, proc, lsp, …) provided by either local handlers or `agentd`.],
  [*Channel*], [A logical bidirectional stream over the transport, with sequence numbers and flow control.],
  [*Daemon*], [The persistent local Codon process; owns sessions and authoritative state.],
  [*Pane*], [A typed leaf in a session's layout; one of terminal, editor, files, diff, git, agent, image, commit.],
  [*Session*], [A unit of context: cwd, capability set, layout, project state. Equivalent to a Zellij workspace.],
  [*Stack*], [A layout node containing N panes in the same slot, one visible at a time. Replaces tabs.],
  [*View protocol*], [The schema set used between window and daemon.],
  [*Window*], [The GUI process; renders only.],
)
