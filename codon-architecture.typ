// Codon — Architecture and Implementation Plan
// Design document v0.2 — scope reduced to single-process Zed fork

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
  #text(size: 11pt, style: "italic")[Design document v0.3 — May 2026]
  #v(2cm)
  #block(width: 75%)[
    #set text(size: 11pt)
    #set par(justify: true, first-line-indent: 0pt)
    #align(left)[
      _Codon is a fork of Zed restructured around a multiplexer-first UX,
      with terminal panes (alacritty-backed) as the default pane kind and
      Helix as the editing model for every text buffer. It runs as a single
      process and reuses Zed's git, agent, diagnostics, diff, and
      commit-editor stacks essentially intact — they are rewired to operate
      over a small `Buffer` trait that Helix's `Document` implements. There
      is no daemon split, no protocol design, no remote agent in this scope.
      Those are deferred. A consistent selection-first / object-verb grammar
      runs across every pane kind: terminal, editor, files, git, agent,
      and the rest._
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

Codon is a single-binary desktop application built by forking Zed and
making three changes:

#set enum(numbering: "1.")

+ *Multiplexer UX.* The default pane kind is a terminal. Sessions
  (workspaces) group panes under a stacked-pane layout (Zellij-style)
  rather than buffer tabs. The window is a multiplexer first; "opening
  the editor" is just spawning an editor pane in the current session.

+ *Helix editing.* Every text buffer in Codon is backed by Helix's
  `Document` and edited with Helix's selection-first command set.
  Zed's editor crate is replaced; Zed's higher-level features that
  consume buffers (git, agent, inline assistant, diff, diagnostics,
  commit editor) are rewired to a small `Buffer` trait that
  `helix_view::Document` implements.

+ *Uniform modal shell.* Every pane — terminal, editor, files, diff,
  git, agent, commit, image — operates under one modal model
  (Normal / Insert / Command), one action registry, one keymap, one
  status/command line. Adding a new pane kind is small and well-defined.

Everything else from Zed is kept as-is: GPUI rendering, alacritty-backed
terminals, picker and fuzzy infrastructure, theme and settings systems,
git plumbing, the agent panel and inline assistant, the commit editor
with AI-generated messages, project diagnostics.

== Design philosophy

Two principles drive the work; the rest follows.

+ *Reuse Zed maximally; replace only what's structurally incompatible.*
  Zed's editor crate is structurally incompatible with using Helix's
  editing model. Everything else in Zed either fits or can be made to
  fit by rewiring buffer dependencies through a trait.

+ *Terminal first, modal everywhere, uniform across surfaces.* Most
  panes are terminals. All panes share one modal model, one action
  registry, one keymap, one status/command line. Adding a new pane
  kind never breaks UX uniformity.

+ *Selection-first / object-verb everywhere it fits.* Every pane that
  exposes typed objects (files, hunks, commits, terminal blocks,
  conversation messages, diagnostics, …) participates in the same
  grammar: select the noun, then apply the verb. The same alphabet
  applies to whatever the focused pane's object set is.
  Section 6 is dedicated to this.

== Goals and non-goals

#table(
  columns: (1fr, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: top,
  table.header(
    [*Goals (this scope)*], [*Non-goals (this scope)*],
  ),
  [Multiplexer-first UX with typed panes],
  [Multi-process daemon architecture],
  [Full Helix editing for every text buffer],
  [Custom protocol design (capnp etc.)],
  [Reuse of Zed's agent, git, diff, diagnostics, commit editor],
  [Remote sessions over SSH; `agentd`],
  [Stacked panes; no buffer tabs],
  [libghostty-vt migration; image preview in terminal panes],
  [Session persistence across app restarts],
  [Mosh-style transport, predictive echo],
  [Yazi-pattern file browser pane],
  [WASM plugins],
  [`Buffer` trait abstraction over Helix and (eventually) other engines],
  [Multi-window],
  [Single-process, single-binary delivery],
  [Cross-platform priority (Linux first; macOS likely; Windows not a goal)],
)

== What is deferred (intentionally)

These things are out of scope for v0 but the design avoids
foreclosing them. They become possible follow-on work.

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left),
  table.header([*Deferred*], [*What it would take when picked back up*]),
  [Daemon split], [Extract sessions/layout/Helix logic into a separate process; design view protocol; first remote-eligible.],
  [Protocol formalization (capnp)], [Comes with the daemon split. Schemas for view protocol (window↔daemon) and capability protocol (daemon↔providers).],
  [Remote sessions over SSH], [Build `agentd`. Capability protocol over SSH `ControlMaster`.],
  [libghostty-vt migration], [Replace `alacritty_terminal` with libghostty-vt via `gpui-ghostty`. Pulls in kitty graphics protocol support, better Unicode, etc.],
  [Mosh-style transport], [UDP roaming + predictive echo on top of the protocol.],
  [WASM plugins], [Plugin runtime; sandboxed pane and capability extensions.],
)

The single largest architectural decision protected by deferring all of
these is that *Codon is a single process for v0*. The internal module
boundaries are clean enough to extract a daemon later, but no IPC
serialization layer is built now.

// ============================================================
= Architecture
// ============================================================

== Single process

Codon is one binary, derived from Zed's. There is no daemon, no remote
agent, no inter-process protocol. The binary's internal structure has
clean module boundaries, but they are linked, not networked.

```
┌──────────────────────────────────────────────────────────┐
│                    codon (single binary)                 │
├──────────────────────────────────────────────────────────┤
│  UX shell — modal panes, status/cmd line, keymap         │
├──────────────────────────────────────────────────────────┤
│  Sessions · Layout · Action registry · Dispatch          │
├──────────────────────────────────────────────────────────┤
│  Pane kinds:                                             │
│    terminal  · editor  · files  · diff                   │
│    git       · agent   · commit · image                  │
├──────────────────────────────────────────────────────────┤
│  Buffer trait                                            │
│    └─ implemented by helix_view::Document                │
├──────────────────────────────────────────────────────────┤
│  Engine layer (libraries, all in-process)                │
│    Zed: gpui, picker, fuzzy, theme, settings, terminal,  │
│         git, agent, inline_assistant, diff, diagnostics, │
│         commit_editor, alacritty_terminal                │
│    Helix: helix-core, helix-view, helix-lsp,             │
│           helix-loader, helix-stdx                       │
└──────────────────────────────────────────────────────────┘
```

What lives in-process: everything. PTYs are spawned with
`portable-pty` (as Zed does today). LSP servers are spawned as child
processes via Helix's `helix-lsp`. Filesystem operations go through
Zed's `fs` crate or direct `std::fs`. There is no abstraction layer
between the kernel and these — they're called directly.

This is the simplification. Internal cleanliness is enforced by Cargo
workspace boundaries, not by serialization seams.

== Internal layering

Module dependencies flow downward only:

```
┌──────────────────────────────────────────────────────────┐
│ ux-shell                                                 │
│  ├── pane host (modal Normal/Insert/Command)             │
│  ├── status & command line                               │
│  └── keymap loader                                       │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│ kernel                                                   │
│  ├── sessions (cwd, layout, project context)             │
│  ├── layout (Split/Stack/Leaf)                           │
│  ├── action registry & dispatch                          │
│  └── persistence to $XDG_STATE_HOME                      │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│ panes-* (one crate per pane kind)                        │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│ codon-buffer (trait)   + vendored Zed crates             │
│                          + vendored Helix crates         │
└──────────────────────────────────────────────────────────┘
```

Pane kinds depend on `codon-buffer`, on the relevant vendored Zed
crates, and on Helix where applicable. The kernel depends only on
panes' types and the action registry. The UX shell depends only on the
kernel and on GPUI rendering primitives.

The pane crates encapsulate all engine-specific dependencies. The
kernel and UX shell never import `helix-*` or Zed crates other than
GPUI directly.

// ============================================================
= Components
// ============================================================

The repository is a single Cargo workspace with three top-level
directories: `vendor/` for forked code we did not author from
scratch, `crates/` for new Codon code, and `apps/` for the binary.

== Vendored from Zed

Forked into the monorepo. We track upstream selectively rather than
automatically; rebases happen on our schedule.

#table(
  columns: (auto, 1fr, auto),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left, center),
  table.header([*Crate*], [*Use*], [*Modified?*]),
  [`gpui`, `gpui-macros`], [Rendering framework. Used as-is.], [no],
  [`picker`, `fuzzy`], [Pickers, nucleo-backed fuzzy search. Powers the command palette and other list UIs.], [no],
  [`theme`, `settings`], [Theme system and settings infrastructure.], [minor],
  [`terminal`, `terminal_view`], [Terminal pane implementation, alacritty-backed. Used as the basis for the terminal pane kind.], [yes — wired into the new pane host],
  [`git` †], [Git status, diff, hunk staging, log, blame.], [yes — rewired to `Buffer` trait],
  [`agent` †], [Conversation history, tool use, MCP integration, agent panel.], [yes — rewired],
  [`inline_assistant` †], [In-buffer AI editing with diff acceptance.], [yes — rewired],
  [`diff` †], [Diff computation and rendering primitives.], [yes — rewired],
  [`diagnostics` †], [Project-level diagnostic aggregation, panel, hover. Replaces Helix's diagnostics.], [yes — rewired],
  [`commit_editor` †], [AI-assisted commit message generation and commit pane.], [yes — rewired],
  [`fs`], [Filesystem abstraction Zed already uses.], [no],
  [`languages`], [Language definitions Zed ships. May be selectively merged with Helix's `languages.toml`.], [tbd],
)

Crates marked † depend on Zed's `Buffer` / `MultiBuffer` / `Project`.
They are forked and re-pointed at Codon's `Buffer` trait
(#ref(<sec:buffer-trait>)). The exact crate names and boundaries in
Zed will need confirmation during fork analysis; the table reflects
the conceptual carve-up.

What we *do not* vendor from Zed: the `editor`, `language`, `text`,
`multi_buffer`, and `rope` crates. These are Zed's editor stack and
are replaced by Helix's. Anything in Zed that imports them transitively
needs to be either rewired (the ones marked †) or excluded.

== Vendored from Helix

```
vendor/helix/
├── helix-core/        # ropey, selections, transactions, syntax
├── helix-view/        # Document, View — used; Editor.tree ignored
├── helix-lsp/         # LSP client
├── helix-loader/      # language config, grammar discovery
└── helix-stdx/        # utility crate Helix depends on
```

We do not vendor `helix-term` (the TUI renderer is replaced by GPUI).
The canonical `Editor` struct in `helix-view` is used for document and
view management but its `Tree` (Helix's pane layout) is not — the
kernel owns layout instead.

== New Codon crates

```
crates/
├── codon-buffer/         # Buffer trait + helix_view::Document adapter
├── kernel/               # sessions, layout, action registry, dispatch
├── ux-shell/             # modal pane host, status/cmd line, keymap loader
├── panes-terminal/       # wraps Zed's terminal_view as a pane kind
├── panes-editor/         # Helix Document rendered through GPUI
├── panes-files/          # yazi-idiom file browser
├── panes-diff/           # uses vendored zed/diff
├── panes-git/            # uses vendored zed/git
├── panes-agent/          # uses vendored zed/agent
├── panes-image/          # local image decode + GPUI render
└── panes-commit/         # uses vendored zed/commit_editor
```

Pane crates depend on the vendored Zed and Helix crates they need; the
kernel and UX shell do not. This is enforced by Cargo workspace
configuration (no path-dep cycles, explicit allow-lists per crate).

== Binary

```
apps/
└── codon/                # the application
```

One binary. Run it; it opens a window. There is no separate daemon to
manage. State is persisted to disk on graceful shutdown and
periodically.

// ============================================================
= Editor split: Helix in editor panes
// ============================================================

<sec:buffer-trait>

== The integration problem

Codon uses Helix's editing model for all text buffers. But it also
reuses Zed's git, agent, inline-assistant, diff, diagnostics, and
commit-editor crates — all of which were written against Zed's own
`Buffer` / `MultiBuffer` / `Project` types. These two stacks are not
API-compatible.

Two options:

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

The trait approach is preferred but the decision is provisional pending
a detailed analysis of how deeply Zed's buffer dependencies penetrate
the crates we want to use. That analysis is part of Phase 3
(#ref(<sec:phase-3>)).

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
consume only this trait; they never see the concrete type.

== Pane crate structure

Pane crates are the only place engine-specific dependencies live.
For `panes-editor`:

```
crates/panes-editor/
├── Cargo.toml             # depends on helix-core, helix-view,
│                          # helix-lsp, codon-buffer, gpui
└── src/
    ├── lib.rs
    ├── document.rs        # owns helix_view::Document; impl Buffer
    ├── commands.rs        # registers Helix commands as actions
    ├── view.rs            # GPUI rendering of the rope + selections
    └── input.rs           # routes keystrokes through Helix's keymap
```

Other pane crates have analogous shapes: a part that owns authoritative
state and a part that handles GPUI rendering. We keep these as separate
modules with disjoint dependencies even though they're in the same
process — when (if) we split out a daemon later, this is the natural
seam.

For pane kinds whose authoritative state comes from a Zed crate
(`panes-git`, `panes-agent`, `panes-inline-assistant`, `panes-commit`),
the implementation pulls in the relevant vendored Zed crate and rewires
its buffer dependencies to `codon_buffer::Buffer`. The rendering side
of these panes can use Zed's existing UI components nearly verbatim,
since the buffer trait abstracts the only divergence.

== Diagnostics: Zed wins

Zed's `diagnostics` crate has a project-level aggregator, a panel,
severity rendering, hover popovers, and code-action integration.
Helix's diagnostic store is simpler. Codon uses Zed's: Helix's LSP
client emits diagnostics, which we forward into Zed's diagnostic store;
both Helix's gutter rendering and Zed's panel/hover read from the same
source.

This means our Helix integration ignores `helix_view::editor::Editor::diagnostics`
and lets Zed's diagnostics crate be canonical.

// ============================================================
= UX shell
// ============================================================

The UX shell is the framework that hosts panes uniformly. It is small
in code but defines the invariants that make Codon feel coherent.

== Modal model

Three modes per pane: *Normal*, *Insert*, *Command*. Mode is per-pane,
not per-window — different panes can be in different modes
simultaneously.

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

== Action registry

```rust
type ActionFn = fn(&mut Kernel, &ActionContext, ActionArgs) -> Result<()>;

pub struct Action {
    pub name: &'static str,
    pub function: ActionFn,
    pub accepts: &'static [ObjectKind],   // empty = nullary verb
}

pub struct Registry {
    actions: HashMap<&'static str, Action>,
}

pub struct ActionContext {
    session: SessionId,
    pane: Option<PaneId>,
    mode: Mode,
    selection: Selection,                  // pulled from focused pane
}
```

The `accepts` field is what makes object-verb work uniformly: each
verb declares which selection kinds it accepts, and the command palette
filters verbs by the focused pane's current selection type. This is
covered in detail in Section 6. For the v0 phases up to Phase 4 most
verbs have `accepts: &[]` (nullary) or single-type accepts; the
generalization is structural rather than ambitious.

Action names are flat strings in dotted namespaces:

```
editor.move_word_forward       editor.extend_word_forward
editor.search                  editor.write
pane.split                     pane.focus.left
pane.stack.add                 pane.stack.cycle
pane.close
session.new                    session.switch
file.open                      file.delete
terminal.new                   terminal.scroll.up
git.diff.current               git.stage.hunk
agent.task.run                 agent.message.send
workspace.command_palette
```

`ActionArgs` is a typed Rust enum (no capnp here — single process, no
serialization). Actions take typed args; the keymap binds keys to
`(name, args)` pairs.

== Keymap layering

Four scopes, applied in order:

#set enum(numbering: "1.")

+ *Global* — works in every mode, every pane. Used for pane navigation,
  session switching, command palette open. These bindings always win.
+ *Per-pane-kind, per-mode* — `[bindings.editor.normal]`,
  `[bindings.terminal.insert]`, etc.
+ *Per-mode* — `[bindings.normal]`. Applies in any pane in that mode if
  no per-pane-kind binding shadows it.
+ *Default* — Helix's keymap for editor modes, hard-coded sensible
  defaults for everything else.

Example:

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
"|" = { action = "pane.split", args = { dir = "right", kind = "terminal" } }
"i" = "pane.mode.insert"

[bindings.editor.normal]
# Helix's full normal-mode keymap, imported verbatim
```

Hot-reloadable. Changes to the config file invalidate the resolver's
cache.

== Status and command line

A single bottom-line widget renders, depending on mode:

- *Normal/Insert:* status — focused pane info, mode indicator, session
  name, optional widgets (git branch, diagnostic counts, current track).
- *Command:* `:` prompt with fuzzy-completing input.

Format strings live in user config, similar to zjstatus. Status content
is sourced from kernel state via lightweight observers (no protocol
needed; same process).

== Pane host

The pane host is the GPUI component that owns a `LayoutNode` and renders
its children. It is recursive:

- A `Split` node renders two child panes side-by-side or stacked, with
  a draggable (keyboard-resizable) separator.
- A `Stack` node renders only the visible pane, with an indicator
  showing position in the stack.
- A `Leaf` node renders a single pane via the appropriate pane crate.

When the focused session changes, the host swaps trees. There is one
pane host per window.

// ============================================================
= Selection-first interaction
// ============================================================

Helix's selection-first model — *the noun is selected and visible
before the verb fires* — generalizes beyond text. Every pane in Codon
has a typed object set; the same modal grammar applied to that set
yields a uniform UX where the same keys mean approximately the same
thing everywhere. This chapter spells out how, what's typed, and which
parts of the generalization land in v0 versus later.

== The principle

Vim is verb-object: `d3w` commits to "delete" before "3 words" is even
visible. Helix is object-verb: select the 3 words, see them
highlighted, then press `d`. The user-facing property is *no surprise*
— the noun is inspectable before the verb fires, and not pressing the
verb undoes nothing.

This property is independent of text. It applies to any UI surface
where (a) there is a stable, typed set of selectable objects, and
(b) verbs operate on selections. Codon enforces it everywhere it makes
sense.

== Object-verb across pane kinds

Each pane kind declares its object types and the verbs that consume
them. A representative cut:

#table(
  columns: (auto, 1fr, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 7pt,
  align: (left, left, left),
  table.header([*Pane*], [*Object types*], [*Representative verbs*]),
  [Editor],
  [Range, Word, Line, Function, Class, Paragraph, Bracket-pair],
  [delete, change, yank, search-replace, format, comment, send-to-agent],
  [Files],
  [File, Directory, Symlink, Pattern-set],
  [delete, copy, rename, chmod, archive, trash, open-in-editor, send-to-agent],
  [Terminal],
  [Block (cmd+output), Line, URL, Path, Process],
  [copy, rerun, fork-pane, send-to-agent, open, follow-path],
  [Git],
  [File, Hunk, Commit, Branch, Stash, Ref],
  [stage, unstage, discard, cherry-pick, revert, rebase-onto, push, send-to-agent],
  [Diff],
  [Hunk, Line, File],
  [stage, revert, send-to-agent],
  [Diagnostics],
  [Diagnostic, File-with-diags, Severity-bucket],
  [goto, suppress, fix-with-code-action, fix-all, send-to-agent],
  [Agent],
  [Message, Code-block, Tool-call, Tool-result, File-mention],
  [branch-from, edit-and-retry, apply-to-buffer, save, re-run, pin],
  [Image],
  [Whole-image, Crop-rect],
  [copy, save, send-to-agent],
  [Commit],
  [Range (msg text), Hunk (staged), File (staged)],
  [generate-message-from, unstage, edit-hunk],
)

A few of these are quietly novel and worth calling out.

*Terminal blocks.* Treating a command together with its output as one
typed object is the Warp/Wave insight. Once blocks exist, "select the
error block, send to agent, fix" replaces the find-copy-paste loop.
Block detection requires shell integration (OSC 133 prompt markers) or
heuristic boundary detection from the terminal stream — solvable but
not free.

*Agent objects.* Treating messages, tool calls, and code blocks in a
conversation as selectable objects turns the chat log from prose into
a navigable artifact. Re-run a tool call with edited arguments. Branch
the conversation from a chosen message. Apply a code block to a buffer.
All by selection plus verb.

*Git objects.* This is the model magit proves out. Codon's
contribution is consistency: the same alphabet that selects words in
the editor selects hunks in git and messages in the agent.

== Typed selections as kernel-aware state

Selections are typed values, not text:

```rust
pub enum ObjectKind {
    Text, File, Dir, Hunk, Commit, Branch, Stash, Ref,
    Block, Url, Path, Process,
    Diagnostic, Message, ToolCall, ToolResult,
    Image, CropRect,
    // …
}

pub enum Selection {
    Empty,
    Text   { buffer: BufferId, ranges: Vec<Range> },
    Files  ( Vec<PathBuf> ),
    Hunks  ( Vec<HunkRef> ),
    Commits( Vec<CommitSha> ),
    Blocks ( Vec<TerminalBlockRef> ),
    Messages( Vec<MessageRef> ),
    Diagnostics( Vec<DiagnosticRef> ),
    // …
    Mixed( Vec<Selection> ),
}

impl Selection {
    pub fn kind(&self) -> Option<ObjectKind> { /* … */ }
}
```

A pane *owns* its current selection. The kernel queries the focused
pane via a small trait:

```rust
pub trait SelectionSource {
    fn current_selection(&self) -> Selection;
    fn object_kinds(&self) -> &'static [ObjectKind];
}
```

Every pane kind implements `SelectionSource`. Action dispatch passes
the focused pane's selection alongside the action context; the
registry's `Action::accepts` field tells the dispatcher (and the
command palette) which actions are valid for the current selection
kind. Invalid verbs are *hidden in the palette*, not errored on the
attempt — the same affordance TypeScript intellisense gives you for
`.`-completions.

This is the load-bearing structural decision: *selection lives at the
kernel boundary, not inside any single pane*. Without it, cross-pane
verbs are impossible and "selection-first" becomes per-pane
re-implementation.

== The selection algebra

Helix's text-Normal-mode movements aren't text-specific in their
shape; they generalize to *selection refinement operators*
parameterized by the pane's object grammar. The same alphabet, the
same muscle memory:

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 7pt,
  align: (left, left),
  table.header([*Helix in text*], [*Generalized refinement*]),
  [`w` next word], [next item of same kind (next file, next hunk, next message, next block)],
  [`mip` inner paragraph], [inner container (containing function, file, thread, severity-bucket)],
  [`s` select-regex within selection], [predicate filter (`s '*.rs'`, `s severity=error`, `s author=carlo`)],
  [`K` keep matching], [intersect with predicate],
  [`A` append cursor], [union with another selection (often via register)],
  [`,` keep first], [reduce to one],
  [`%` whole-buffer], [select all of pane's objects],
  [`n` / `N` repeat search], [next/prev predicate match],
)

These are not separate per-pane keymaps; they are the same operators
realized over each pane's grammar. The user learns the alphabet once,
and "extend selection to next thing" works in seven panes for one set
of muscle memory.

The grammar is thin. Each pane crate exposes a small trait:

```rust
pub trait ObjectGrammar {
    fn next(&self, kind: ObjectKind, from: &Selection) -> Selection;
    fn inner_container(&self, of: ObjectKind, from: &Selection) -> Selection;
    fn filter_by_predicate(&self, sel: &Selection, p: Predicate) -> Selection;
    // …
}
```

The UX shell drives Normal-mode keys through this trait. Implementing
it for a new pane kind is the cost of giving that pane the full
Helix-shaped navigation model.

== Cross-pane verbs

Verbs that accept multiple object kinds unify workflows across panes:

```
agent.explain    accepts: [Text, Hunk, Block, Diagnostic, Message]
agent.fix        accepts: [Diagnostic, Block]  // block = error output
diff.against     accepts: [File, Commit, Branch]
git.blame.show   accepts: [Text, File]
fs.show          accepts: [Path, FileMention]  // from agent output
```

Concrete workflows that drop out:

- *Files pane:* select 12 files matching `*.ts`. Trigger
  `agent.explain` directly (no pane switch needed). The agent receives
  a `Files` selection and treats it as its working set.
- *Diagnostics panel:* select all `TS2345` errors across the project.
  Trigger `agent.fix`. The agent receives a `Diagnostics` selection
  and produces a single patch.
- *Git log:* select 3 commits. Trigger `agent.write_release_notes`.
  The agent receives `Commits`.
- *Terminal:* select the block containing a stack trace.
  `agent.explain`. The block's command and output become the agent's
  context.

The user binds one key for `agent.explain` once and it works in every
applicable context. This is the primary payoff of object-verb being a
kernel-level concept rather than per-pane.

== Selection registers

Helix's registers (`"a`, `"b`, …) hold yanked text. Generalized: any
typed selection can be stored in a register. Registers persist for the
session; named registers (declared in config) persist across sessions.

```
"f          "files I always review together"  → Files(...)
"e          last-error set                    → Diagnostics(...)
"c          cherry-pick candidates            → Commits(...)
```

Verbs that produce selections (`select-pattern`, `select-by-author`,
or the result of an agent query) can write into registers. Verbs that
consume selections can read from them: `"f open` opens all files in
register `f`; `"e fix` fixes all diagnostics in register `e`.

Registers are the pipeline-composition mechanism: they make the user's
working set inspectable, named, and reusable instead of fleeting.

== Where the model doesn't fit

An honest list of cases where forcing object-verb is wrong:

- *Insert mode in editor and terminal.* No noun is being constructed;
  characters are being emitted. Insert mode is the escape hatch and
  isn't subject to the model.
- *Configuration verbs.* "Toggle word wrap," "increase font size."
  No object. These live in Command mode (`:`) as nullary verbs in
  the registry (`accepts: &[]`).
- *Verb arguments that aren't selection-shaped.* `:write some/path.rs`
  takes a string, not a noun in the grammar. Command mode has free-form
  arguments. We don't pretend everything is selection-shaped.
- *Continuous mouse-style operations.* Drag-resize, scrub a scrollbar.
  Awkward in object-verb. The few mouse-friendly affordances live
  outside the modal grammar.

The principle: *selection-first wherever there's a stable typed object
set; Command mode for the rest.* Don't force everything into the
noun-verb model when the verb is genuinely arity-1 and the argument
isn't selection-shaped.

== v0 scope and the path to full generality

Full kernel-level typed selection with the algebra and registers is
more ambition than v0 requires. The phase compromise:

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 7pt,
  align: (left, left),
  table.header([*Phase*], [*Selection-first scope*]),
  [Phase 1], [Editor pane only — i.e., Helix's standard model verbatim. Other panes are pre-grammar; their actions are pane-local.],
  [Phase 3], [Files pane and git pane participate. `Selection::Files`, `Selection::Hunks`, `Selection::Commits` defined. First cross-pane verb (e.g. `git.blame.show` accepting `Text | File`).],
  [Phase 4], [Diagnostics, Diff, Image. Refinement operators (`w`, `mip`-style) implemented for each pane's grammar.],
  [Phase 5], [Agent and Terminal. Block detection (OSC 133 if available, heuristics otherwise). `agent.explain` becomes the universal cross-pane verb.],
  [Post-v0], [Selection algebra completion (`s` predicate filter syntax, `K` intersect). Registers. Named persistent registers. Full object-grammar trait implementation across all panes.],
)

The architectural commitment in v0 is *not* deferring this — it is
*shaping the interfaces now so the generalization is additive later*.
Specifically:

#set enum(numbering: "1.")

+ `Action::accepts` is in the registry from day one (Phase 1), even
  though most early verbs have empty or single-type accepts.
+ `SelectionSource` and `ObjectGrammar` traits are declared in the
  kernel's pane interface from Phase 1, even if early implementations
  return placeholder values.
+ The command palette's filtering by selection kind is implemented from
  Phase 1 (filtering an empty-or-single-type selection set is trivial).

The cost of doing this correctly from day one is small; the cost of
retrofitting it is high. This is the one place the "single process,
single binary" simplification of v0 doesn't slacken — the abstractions
have to be right early because they shape what the UX feels like at the
keyboard.

// ============================================================
= Sessions and layout
// ============================================================

== Sessions

```rust
pub struct Session {
    id: SessionId,
    name: String,
    cwd: PathBuf,
    layout: LayoutNode,
    panes: HashMap<PaneId, PaneKind>,
    project: ProjectContext,    // built lazily
    created: Instant,
    last_attached: Option<Instant>,
}

#[derive(Default)]
pub struct ProjectContext {
    root: OnceCell<Option<PathBuf>>,
    git: OnceCell<Option<GitRepo>>,
    lsp_clients: HashMap<LanguageId, LspClientHandle>,
    diagnostics: DiagnosticStore,
}
```

Sessions are workspaces. There is no separate "is this a workspace"
concept; project context accretes lazily as features are used.

A new session is cheap: a name, a cwd, an empty layout with one
terminal pane. Project context starts empty.

Project context accretes lazily, triggered by the first pane that needs
it:

- *First editor pane opens.* The kernel walks up from `cwd` looking for
  language manifests and `.git`. If found, `project.root` is set.
  An LSP client for the file's language is spawned via `helix-lsp`.
- *First git pane opens.* `project.git` is set if cwd is inside a git
  work tree.
- *First diagnostic arrives.* From any LSP client, gets stored in
  `project.diagnostics`, consumed by the diagnostics panel, the editor
  gutter, and the agent.

A session never un-promotes. Starting fresh is essentially free; using
an existing session inherits its warmed-up state.

== Layout

Three node types, no buffer tabs:

```rust
pub enum LayoutNode {
    Split { dir: Direction, ratio: f32, a: Box<LayoutNode>, b: Box<LayoutNode> },
    Stack { panes: Vec<PaneId>, visible: usize },
    Leaf(PaneId),
}

pub enum Direction { Horizontal, Vertical }
```

A session has exactly one `LayoutNode` as root. Stacks substitute for
tabs: when you want N variants in the same slot, stack them and cycle
visibility with `pane.stack.cycle`.

Replacing Zed's existing tab-strip UI with stack indicators is the
biggest UI surgery in the project; it is concentrated in one place
(the pane host).

== Persistence

Sessions persist across application restarts. State is written to disk:

- *Periodic snapshots* every 30 seconds.
- *Graceful shutdown* writes a final snapshot.

Persisted per session: metadata (name, cwd, creation time), layout
tree, per-pane minimal state.

Per-pane handling on rehydrate:

- *Terminal:* PTY does not survive process restart; on rehydrate, the
  pane is replaced with a placeholder showing the last 1 MiB of
  scrollback and a "press Enter to respawn" prompt.
- *Editor:* open file path and view state; rope content is re-read.
  Unsaved changes are persisted as a swap file.
- *Files / git / diff / image:* enough state to recreate.
- *Agent:* full conversation history (Zed's agent crate already
  persists this; we use its mechanism).
- *Commit:* in-progress commit message text.

== Session list and switching

The session list is a picker (vendored Zed `picker` + `fuzzy`), opened
by `session.switch`. It shows session name, cwd, last-attached time,
and a preview snapshot of the layout.

`session.new` opens a "create session" flow with a cwd field. There is
no current-session hierarchy beyond what the user sees in the picker;
one session is shown in the window at a time.

// ============================================================
= Implementation plan
// ============================================================

Sequential phases, each ending in a usable artifact.

== Phase 0 — Walking skeleton

*Goal:* A Zed fork that builds, opens a window with one terminal pane
(alacritty-backed) and one editor pane that renders Helix's `Document`
through GPUI.

Build steps:

- Set up the monorepo. Fork Zed wholesale into `vendor/zed/`. Vendor
  Helix (`helix-core`, `helix-view`, `helix-lsp`, `helix-loader`,
  `helix-stdx`) into `vendor/helix/`.
- Strip out (or stub) Zed's `editor`, `language`, `text`,
  `multi_buffer`, and `rope` crates from the build. The app won't
  build yet; that's fine.
- Build a minimal `panes-editor` crate that holds a
  `helix_view::Document`, exposes its rope through GPUI's text
  rendering, and renders selections.
- Build a minimal `panes-terminal` that wraps Zed's existing
  `terminal_view`.
- Replace Zed's main workspace with a minimal pane host that opens one
  terminal pane and one editor pane side-by-side. Hardcode the editor
  to open `/etc/hostname` or similar; read-only.

*Deliverable:* `cargo run` opens a window with shell + Helix-rendered
file. Nothing is interactive in the editor pane yet.

== Phase 1 — Action layer and modal shell

*Goal:* Helix's full editing model works in editor panes; pane focus,
splits, and command palette work uniformly across pane kinds; keymap
is configurable.

Build steps:

- Build `crates/ux-shell` with the modal pane host (Normal / Insert /
  Command), status/command line, keymap loader.
- Build `crates/kernel` minimally: action registry only. No sessions
  yet.
- Wire Helix's command set into the registry under `editor.*`.
  Generate registration from Helix's existing command tables.
- Add cross-cutting actions: `pane.focus.*`, `pane.split.*`,
  `pane.close`, `workspace.command_palette`, `editor.write`.
- Implement the command palette using vendored `picker`/`fuzzy`.
- Implement the TOML keymap loader; ship a default keymap that imports
  Helix's normal-mode bindings verbatim.
- Define the `Selection`, `ObjectKind`, `SelectionSource`, and
  `ObjectGrammar` interfaces (Section 6). Register actions with
  `accepts` (mostly empty in this phase). The command palette filters
  by selection kind. Editor pane is the only `SelectionSource`
  implementation; others return `Selection::Empty`.

*Deliverable:* a one-window Codon with a working Helix editor (full
keymap, modal model, save), working terminal, working command palette.
No sessions yet; one fixed layout.

== Phase 2 — Sessions, layout, persistence

*Goal:* multiplexer UX. Sessions exist, can be created, switched,
persisted. Stacked panes work. Default new pane is terminal.

Build steps:

- Add session management to the kernel: `Session`, layout tree,
  pane registry.
- Replace the fixed layout with the recursive `LayoutNode` host.
- Implement stacked panes. Repurpose or strip Zed's tab strip in favor
  of stack indicators.
- Add session actions: `session.new`, `session.switch`, `session.list`.
  Wire the session picker.
- Implement disk persistence: periodic snapshots, graceful-shutdown
  write, rehydrate on launch. Terminal placeholders on rehydrate.
- Add `pane.split` parametric action; default kind = terminal.
- Verify: open Codon → land in last session → close window → reopen →
  layout intact, scrollback shown, terminals respawnable.

*Deliverable:* Codon as a multiplexer. Same shape as Zellij/tmux for
terminal usage, but with Helix-backed editor panes available.

== Phase 3 — Buffer trait + git rewire

<sec:phase-3>

*Goal:* the `Buffer` trait is in place; Zed's git crate is forked,
rewired, and integrated as the first buffer-consuming feature.

Build steps:

- *Analyze fork impact:* survey Zed's `git` crate for buffer
  dependencies. Decide trait-vs-wrapper based on what we find.
  Update this design doc with the decision before proceeding.
- Define `crates/codon-buffer` with the `Buffer` trait. Implement it
  for `helix_view::Document`.
- Fork Zed's `git` crate into `vendor/zed/git/`. Rewire its buffer
  dependencies. Wire it into Codon as the `panes-git` crate.
- Add hunk-staging actions; verify integration with editor panes.

*Deliverable:* git pane works. Status, log, diff, hunk staging all
function over Helix-backed buffers. The trait abstraction is proven.

Risk: the git rewire reveals deeper buffer dependencies than the trait
can express. Mitigation: the analysis step is a real deliverable; if
the trait can't bridge, we adopt the wrapper approach and revise.

== Phase 4 — Native UX coverage

*Goal:* file browser, diff viewer, image preview panes work.
Diagnostics are wired in.

Build steps:

- Build `panes-files`: yazi-idiom three-column file browser. Modal:
  Normal for navigation, Insert for fuzzy filter, Command for `:` actions.
- Build `panes-diff`: integrates Zed's vendored diff crate, rewired.
- Build `panes-image`: local image decode (image crate) + GPUI render.
- Fork and rewire Zed's `diagnostics` crate. Replace Helix's
  diagnostics throughout. Editor gutter and the diagnostics panel
  consume the same store.
- Add a status-line widget for diagnostic summary.

*Deliverable:* coherent local IDE. File browse, edit, diff, view,
diagnose, all session-aware.

== Phase 5 — Agent, inline assistant, commit editor

*Goal:* AI-augmented coding works. Conversations persist. Inline edits
roundtrip. Git commit messages can be auto-generated.

Build steps:

- Fork Zed's `agent`, `inline_assistant`, and `commit_editor` crates;
  rewire to `Buffer`.
- Build `panes-agent` and `panes-commit`. Verify conversation
  persistence works through Codon's session persistence.
- Wire the inline assistant to the editor pane: a binding triggers a
  ranged AI edit; the resulting diff is shown inline; the user accepts
  or rejects per hunk.
- Wire commit-message auto-generation in the commit pane.
- Add agent-aware status-line widgets (running task indicator,
  notification on completion).

*Deliverable:* feature parity with Zed for local agentic coding, plus
the multiplexer-first UX and Helix editing.

== Summary of phases

#table(
  columns: (auto, 1fr, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left, left),
  table.header([*Phase*], [*Outcome*], [*Shippable as*]),
  [0], [Zed fork builds; Helix Document renders in a pane.], [internal demo],
  [1], [Helix editing + modal shell + command palette.], [single-binary "graphical Helix in a Zed shell"],
  [2], [Sessions, stacked panes, terminal-first layout, persistence.], [Codon v0.1 — usable as a multiplexer with editor pane],
  [3], [Buffer trait + git pane.], [Codon v0.2],
  [4], [Files, diff, image, diagnostics.], [Codon v0.3],
  [5], [Agent, inline assistant, commit editor.], [Codon v0.4 — feature-complete v0],
)

// ============================================================
= Open questions and decisions deferred
// ============================================================

== Confirmed for v0

- Single process, single binary. No daemon, no protocol, no remote.
- Project name: *Codon*.
- Editing model: *Helix* everywhere there's a text buffer.
- Terminal backend: *alacritty_terminal*, as Zed uses today.
- Reuse: maximum of Zed's product features; minimum of its editor stack.
- Layout: Split / Stack / Leaf. No tabs.
- Default pane kind: terminal.
- Display model: one session shown at a time.
- *Selection-first / object-verb across panes.* Interfaces
  (`Selection`, `ObjectKind`, `SelectionSource`, `ObjectGrammar`,
  `Action::accepts`) are declared from Phase 1; full grammar
  implementation rolls out per pane in Phases 3–5; selection algebra
  and registers are post-v0. See Section 6.
- Multi-window, WASM plugins, predictive echo: deferred.

== Provisional / to be analyzed

- *Buffer trait vs wrapper.* Phase 3 includes a survey of Zed's
  buffer-dependent crates. Decision provisional pending that analysis.
- *Helix gutter rendering.* Helix's gutter (line numbers,
  diagnostic markers, change indicators) is currently TUI-rendered.
  We need to either reimplement it as a GPUI component reading from
  Codon's diagnostic store and Helix's `Document` state, or delegate
  to vendored Zed gutter rendering. Likely the former; decide in
  Phase 1.
- *vim-style shell hook.* Detecting `vim foo.rs` in a terminal and
  routing to an editor pane — opt-in, not blocking. Design in Phase 4.
- *Settings storage and format.* Zed uses JSON; Helix uses TOML.
  Codon is TOML-first (matches keymap). Decide where the settings file
  lives and how per-project settings overlay user settings.
- *Theme system.* Zed's theme infrastructure works; Helix has its own
  themes. We use Zed's; Helix's TOML themes need to be importable.
- *Languages config merge.* Zed ships its own languages list; Helix
  ships `languages.toml`. Probably we want Helix's (since it drives
  Helix's syntax), with Zed's as supplementary. Decide in Phase 1.
- *Terminal block detection.* Object-verb in the terminal pane
  (Section 6) requires identifying command-and-output as one unit.
  Two strategies: OSC 133 prompt markers (requires shell integration —
  user opt-in) and heuristic boundary detection from the byte stream
  (works for everyone but lossy). Likely both, with OSC 133 preferred
  when present. Decide in Phase 5.

== Deferred to v0.next (with notes)

The architecture is intentionally daemon-extractable. When (if) we pick
these up, here is the rough cost.

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left),
  table.header([*Deferred*], [*Approximate cost when picked up*]),
  [Daemon split], [Extract `kernel` and the state-owning halves of `panes-*` into a `codon-daemon` binary. Define view protocol schemas. The pane-crate split into "owns state" vs "renders" was set up to make this tractable.],
  [Capnp protocol formalization], [Comes with the daemon split. Two schema sets; one channel multiplexer over a Unix socket.],
  [Remote sessions over SSH], [Build `agentd` (~4 kLOC). Capability protocol over SSH ControlMaster. Per-session capability binding (local OR remote).],
  [libghostty-vt migration], [Replace `alacritty_terminal` with `libghostty-vt` via `gpui-ghostty`. Pulls in better Unicode, kitty graphics protocol parsing. Some work in `panes-terminal`; small surface.],
  [Mosh-style transport], [UDP-based transport with state replay. Predictive echo for editor and terminal panes.],
  [WASM plugin runtime], [A `plugin` capability and an in-app sandboxed WASM host. Plugins can register actions, define pane kinds, or proxy capabilities.],
)

The single largest design decision protected by deferring all of these
is that Codon stays one process for v0. Doing the daemon extraction
later means we ship something usable sooner; doing it now would be the
right architecture for the eventual full vision but a much longer road
to a working system.

== Out of scope

- Cross-platform priority. Linux first; macOS likely; Windows is not
  a goal and may never be one.
- Web client.
- Collaboration (multi-user editing).
- Browser-based remote terminal access.

// ============================================================
= Appendix: glossary
// ============================================================

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 6pt,
  align: (left, left),
  [*Action*], [A named operation in the kernel's registry; invoked by keymap or command palette. Carries an `accepts: &[ObjectKind]` declaration.],
  [*Buffer trait*], [The small Rust trait that abstracts buffer access. Implemented by `helix_view::Document`; consumed by vendored Zed crates after rewire.],
  [*Object grammar*], [A pane kind's declaration of its object types and the refinement operators (next, inner, filter, …) over them.],
  [*ObjectKind*], [The discriminant of `Selection` — Text, File, Hunk, Commit, Block, Message, Diagnostic, etc. Used by `Action::accepts` to gate verb applicability.],
  [*Pane*], [A typed leaf in a session's layout; one of terminal, editor, files, diff, git, agent, image, commit.],
  [*Pane host*], [The GPUI component that recursively renders a `LayoutNode`.],
  [*Register*], [A named slot holding a typed `Selection`. Per-session by default; named registers persist across sessions.],
  [*Selection*], [A typed value representing the currently-targeted nouns in the focused pane. Pulled into action context by the kernel via `SelectionSource`.],
  [*Session*], [A unit of context: cwd, layout, project state. The Codon equivalent of a Zellij workspace.],
  [*Stack*], [A layout node containing N panes in the same slot, one visible at a time. Replaces tabs.],
  [*Vendored*], [Forked into Codon's monorepo under `vendor/`. We modify freely; we re-base from upstream on our schedule.],
)
