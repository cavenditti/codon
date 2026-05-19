// Codon — Architecture and Implementation Plan
// Design document v0.4 — reflects shipped phases 1–13 and in-flight 14–17

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
  #text(size: 11pt, style: "italic")[Design document v0.4 — May 2026]
  #v(2cm)
  #block(width: 75%)[
    #set text(size: 11pt)
    #set par(justify: true, first-line-indent: 0pt)
    #align(left)[
      _Codon is a fork of Zed restructured around a multiplexer-first UX,
      with terminal panes (alacritty-backed) as the default pane kind and
      always-on Helix-style modal editing for every text buffer. It runs
      as a single process and reuses Zed's editor, git, agent,
      diagnostics, diff, and commit-editor stacks essentially intact. The
      modal experience comes from Zed's built-in vim mode with the Helix
      defaults force-enabled, not from a separate engine — Helix is the
      reference experience, not a vendored runtime. A consistent
      selection-first / object-verb grammar runs across every pane kind:
      terminal, editor, file manager, git, agent, and the rest._
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

+ *Multiplexer UX.* The default pane kind is a terminal. The window is
  a multiplexer first; "opening the editor" is just spawning an editor
  pane in the current session. Sessions group windows tmux-style: each
  session is a (name, cwd, list-of-windows) tuple, each window holds
  its own pane layout, one window is visible at a time.

+ *Always-modal shell.* Every pane — terminal, editor, file manager,
  diff, git, agent, commit, image, debug, outline — runs under one
  modal model (Normal / Insert / Command), one action registry, one
  TOML keymap, one status/command line. The editing model itself is
  Zed's built-in vim mode with the Helix default profile force-enabled;
  no separate editor engine is vendored.

+ *Selection-first across surfaces.* Every pane that exposes typed
  objects (files, hunks, commits, terminal blocks, conversation
  messages, diagnostics, …) participates in the same grammar: select
  the noun, then apply the verb. The same alphabet applies to whatever
  the focused pane's object set is. Section 6 is dedicated to this.

Everything else from Zed is kept as-is: GPUI rendering,
alacritty-backed terminals, picker and fuzzy infrastructure, theme and
settings systems, git plumbing, the agent panel and inline assistant,
the commit editor with AI-generated messages, project diagnostics.
Adapter code (`codon-panes`) hosts Zed's `Panel` impls as first-class
workspace panes rather than dock-hosted sidebars.

== Design philosophy

Three principles drive the work; the rest follows.

+ *Reuse Zed maximally; add only what's missing.* Zed already ships a
  Helix-shaped vim mode, an agent, a git stack, an inline assistant,
  and a commit editor. Codon adds the multiplexer shell on top, rewires
  dock-hosted panels as panes, and supplies the selection-first
  action layer. No editor stack is replaced.

+ *Terminal first, modal everywhere, uniform across surfaces.* Most
  panes are terminals. All panes share one modal model, one action
  registry, one keymap, one status/command line. Adding a new pane
  kind never breaks UX uniformity.

+ *Keyboard-first, always.* Codon is driven entirely by the keyboard.
  Mouse-only affordances from Zed (tab close "x" buttons, hover-only
  icons, click-to-do-anything controls) are stripped when a keybinding
  covers the same action. If a verb has no binding yet, the answer is
  to add the binding to the TOML defaults — not to leave a mouse
  control in place.

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
  [Always-modal editing via Zed's vim + `helix_default`],
  [Vendoring helix-core / helix-view as an editor engine],
  [Reuse of Zed's editor, agent, git, diff, diagnostics, commit editor],
  [Custom protocol design (capnp, IPC, etc.)],
  [tmux-style sessions and windows; one OS window],
  [Multi-OS-window],
  [Tabs-as-windows today; nested stacked panes as a deferred enhancement],
  [Remote sessions over SSH; `agentd`],
  [Yazi-pattern file manager pane],
  [WASM plugins],
  [Unified TOML config (keymap + chord prefix + per-feature settings)],
  [Mosh-style transport, predictive echo],
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
  [First-class stacked panes],
  [The `LayoutSnapshot::Stack` variant exists but falls back to its
   active member on apply. Live `Member::Stack` rendering needs a
   refactor of vendored Zed's `pane_group` (~15 match sites). Worth
   it when nested layouts become routine; today's tabs cover the
   common case.],
  [Daemon split],
  [Extract sessions/layout/action-registry logic into a separate
   process; design view protocol. The `codon-*` crate boundaries are
   the natural seam.],
  [Protocol formalization (capnp)],
  [Comes with the daemon split. Schemas for view protocol
   (window↔daemon) and capability protocol (daemon↔providers).],
  [Remote sessions over SSH],
  [Build `agentd`. Capability protocol over SSH `ControlMaster`.],
  [Mosh-style transport],
  [UDP roaming + predictive echo on top of the protocol.],
  [WASM plugins],
  [Plugin runtime; sandboxed pane and capability extensions.],
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
│  codon UX layer                                          │
│    codon-keymap   · codon-pickers  · codon-jump          │
│    codon-mode     · codon-command-palette                │
├──────────────────────────────────────────────────────────┤
│  codon kernel                                            │
│    codon-session     (sessions + windows + runtime cache)│
│    codon-pane-bridge (PaneMode + CodonModeTracker)       │
│    codon-config      (unified TOML loader + writeback)   │
├──────────────────────────────────────────────────────────┤
│  codon panes                                             │
│    codon-panes  (PanelItemAdapter over Zed's Panel trait)│
│    file-manager (yazi-style, codon-native)               │
│    codon-agent  (cross-pane verbs, selection seeding)    │
├──────────────────────────────────────────────────────────┤
│  Vendored Zed (workspace member; branch `codon`)         │
│    gpui · picker · fuzzy · theme · settings · workspace  │
│    editor (+ vim crate with helix_default)               │
│    terminal_view (alacritty_terminal)                    │
│    git_ui · agent_ui · diagnostics · debugger_ui         │
│    project_panel · outline_panel · ...                   │
└──────────────────────────────────────────────────────────┘
```

What lives in-process: everything. PTYs are spawned with
`portable-pty` (as Zed does today). LSP servers are spawned as child
processes by Zed's language stack. Filesystem operations go through
Zed's `fs` crate or direct `std::fs`. There is no abstraction layer
between the kernel and these — they're called directly.

Internal cleanliness is enforced by Cargo workspace boundaries, not by
serialization seams.

== Internal layering

Module dependencies flow downward only. The codon crates sit above
vendored Zed; downstream codon crates never import each other except
through the explicit pane-bridge / config / session APIs.

```
┌──────────────────────────────────────────────────────────┐
│ codon UX                                                 │
│  ├── codon-keymap (TOML loader + cheatsheet)             │
│  ├── codon-command-palette (Helix-style `:` palette)     │
│  ├── codon-pickers (shared ModalScaffold)                │
│  ├── codon-jump (Vimium-style hint overlay)              │
│  └── codon-mode (mode_indicator + re-exports)            │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│ codon kernel                                             │
│  ├── codon-session  (Session, Window, WindowRuntimeCache)│
│  ├── codon-pane-bridge (PaneMode + CodonModeTracker      │
│  │    + PaneModeBridge trait — cycle-free base)          │
│  └── codon-config (unified TOML, toml_edit writeback)    │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│ codon panes                                              │
│  ├── codon-panes   (PanelItemAdapter — one wrapper for   │
│  │    every Zed Panel impl)                              │
│  ├── file-manager  (codon-native FM pane)                │
│  └── codon-agent   (cross-pane verbs)                    │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│ vendored Zed (single workspace member)                   │
│  workspace::codon_bridge exposes LayoutSnapshot +        │
│  capture_layout / apply_layout / replace_center_with_…   │
│  + the unified codon_register_pane_kind registry         │
└──────────────────────────────────────────────────────────┘
```

`codon-keymap` deliberately does *not* depend on any downstream codon
crate. Each owning crate registers its own GPUI actions from its own
`init(cx)`; the keymap resolves names through the global action
registry only. This keeps the dependency graph acyclic and the keymap
hot-reloadable without touching feature crates.

// ============================================================
= Components
// ============================================================

The repository is a single Cargo workspace. Three top-level
directories: `vendor/` for forked submodules, `crates/` for codon
crates, and `apps/` for the binary.

== Vendored from Zed

A git submodule at `vendor/zed/` tracking a `codon` branch. Every
vendored Zed crate is a Cargo workspace member, so modifications
compile and link directly. We follow Zed's upstream conventions when
editing here (no `unwrap()`; no silent `let _ =`; prefer additive
changes to existing files; `./script/clippy`).

#table(
  columns: (auto, 1fr, auto),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left, center),
  table.header([*Crate*], [*Use*], [*Modified?*]),
  [`gpui`, `gpui-macros`],
  [Rendering framework. Codon adds `set_keystroke_chord_timeout` for
   multi-key chord support.],
  [minor],
  [`picker`, `fuzzy`],
  [Pickers, nucleo-backed fuzzy search. Powers the command palette,
   session/window switchers, and most codon modals.],
  [no],
  [`theme`, `settings`],
  [Theme system and settings infrastructure.],
  [minor],
  [`workspace`],
  [Workspace, panes, docks. Codon adds `codon_bridge` (LayoutSnapshot
   + the unified pane-kind registry), `replace_center_with_empty_pane`,
   `restore_center_root`, `serialize_workspace_now`.],
  [yes],
  [`editor`, `vim`],
  [Zed's editor with the vim crate; codon force-enables the
   `helix_default` profile so Helix-style modal editing is on by
   default for every text buffer.],
  [minor],
  [`terminal_view`, `terminal`, `alacritty_terminal`],
  [Terminal pane implementation. Terminals are panes in codon, not
   docked.],
  [yes — modal wiring],
  [`git_ui`],
  [Git status, diff, hunk staging, log, blame. Hosted as a codon pane
   via `codon-panes`.],
  [yes — adapter hosting],
  [`agent_ui`, `agent`],
  [Conversation history, tool use, MCP integration, agent panel.
   Surfaces a `seed_explain_with_selection` entry point for codon's
   cross-pane verbs.],
  [yes — selection seeding],
  [`inline_assistant`],
  [In-buffer AI editing with diff acceptance.],
  [no],
  [`diagnostics`],
  [Project-level diagnostic aggregation, panel, hover.],
  [no],
  [`outline_panel`, `debugger_ui`, `project_panel`],
  [All hosted as codon panes via `codon-panes` (Phase 12).
   `project_panel` is superseded at runtime by `file-manager`.],
  [yes — adapter hosting],
)

The vendored Zed branch carries codon-specific surface additions where
they are unavoidable; everything else stays upstream-compatible to keep
periodic rebases cheap.

== Codon crates

```
crates/
├── codon-pane-bridge/     # PaneMode + CodonModeTracker + PaneModeBridge
├── codon-mode/            # re-exports pane-bridge + mode_indicator
├── codon-keymap/          # TOML loader + cheatsheet + chord prefix
├── codon-session/         # sessions + windows + in-memory pane stash
├── codon-panes/           # PanelItemAdapter for Zed Panel impls
├── codon-pickers/         # shared ModalScaffold for codon modals
├── codon-command-palette/ # Helix-style `:` palette
├── codon-jump/            # Vimium-style hint overlay
├── codon-agent/           # cross-pane agent verbs
├── codon-config/          # unified ~/.config/codon/codon.toml
└── file-manager/          # yazi-style three-column file manager
```

Each crate's lib-root file matches the Cargo package's underscored
form (`crates/codon-session/src/codon_session.rs` for package
`codon-session`). Types exported from a crate carry the crate's
vocabulary in the name (`KeymapCheatTab`, not `CheatTab`; `JumpKind`,
not `Kind`).

== Binary

```
apps/
└── codon/                # the application
```

One binary. Run it; it opens a window. State is persisted to the
global KVP store (`codon_sessions_v1`) on every mutation via a
background spawn, plus a 30-second heartbeat.

== Vendored helpers (forge-spec)

A second submodule at `vendor/forge-spec/` provides the `spec` CLI for
roadmap tracking. The `.specs/` directory at the repo root holds the
TOPIC/REQ/TASK files; see `.specs/AGENTS.md` for the vocabulary.

// ============================================================
= Editor model
// ============================================================

== Helix-style without vendoring Helix

Codon does not vendor `helix-core`, `helix-view`, or any other
helix-editor crate as a runtime engine. The Helix experience comes
from Zed's built-in vim mode with the `helix_default` profile
force-enabled at startup. Every text buffer in codon is a
`language::Buffer`; selections, registers, multi-cursor, and
object-mode operators all come from the vim crate's Helix code path.

This was a deliberate pivot. The original v0.3 plan called for
plugging `helix_view::Document` into codon as a second buffer backend
via a `codon_buffer::Buffer` trait, with consumers (editor, search,
agent, git) rewired to `&dyn Buffer`. That plan was *superseded* on
2026-05-13:

- Zed's vim crate already covered ~80% of Helix's normal/select-mode
  keymap with the `helix_default` profile.
- The remaining 20% is small enough to backfill as TOML bindings and
  targeted additions to the vendored vim crate (Phase 16 covers
  shell-pipe verbs, jumplist pickers, changed-files picker, and the
  cheatsheet entries).
- With no second consumer ever planned, the `codon_buffer` abstraction
  had no payoff over using `language::Buffer` directly.

The `crates/codon-buffer/` crate exists from earlier exploration but
has zero consumers and is slated for removal. The `vendor/helix/`
submodule stays in tree as *reference material* — when we want to
understand how Helix implements a behavior we're mirroring, the
source is right there. It is not built or linked.

== Reference, not runtime

Helix's role in codon is as the *reference experience*: when a Helix
behavior is missing from Zed's vim mode (shell-pipe `|`/`!`/`Alt-|`/
`Alt-!`/`$`, `gw` jump-to-word, jumplist picker, etc.), the bug is in
codon — we either teach the TOML defaults, register a new action in
the vendored vim crate, or build a codon-native overlay that mirrors
the Helix UX. We do not import helix-editor code to fix it.

== Diagnostics, search, LSP

All three come from Zed unmodified. Diagnostics flow into Zed's
`diagnostics` crate; the editor gutter, the diagnostics panel, and the
agent all read from the same store. The diagnostics pane is hosted via
`codon-panes` like any other Zed panel.

// ============================================================
= UX shell
// ============================================================

The UX shell is the framework that makes every pane feel coherent. It
is small in code but defines the invariants that hold the experience
together.

== Modal model

Three modes per pane: *Normal*, *Insert*, *Command*. Mode is per-pane,
not per-window — different panes can be in different modes
simultaneously. The global `CodonModeTracker` reflects the focused
pane's mode and drives the status-bar indicator.

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left),
  table.header([*Mode*], [*Behavior*]),
  [Normal],
  [Keys execute named actions according to the keymap. Default for
   editor, file manager, git, agent, diff, image, debug, outline.
   Pane-kind-specific keymap section applies.],
  [Insert],
  [Keys reach the pane's underlying widget. In a terminal, raw bytes
   to the PTY. In an editor, vim insert-mode editing (with Helix
   defaults). In the file manager, fuzzy-filter input. Default for
   terminal panes only.],
  [Command],
  [`:` opens a Helix-style command line at the bottom; tab completes
   against the action registry. Action invoked on Enter. Same in
   every pane. Terminal panes enter command mode via double-Esc,
   which also pauses PTY writes and enables alacritty's vi mode for
   cursor motion and selection.],
)

The `PaneModeBridge` trait owns the mode-tracker invariant: a single
focus subscriber installed via `install_pane_mode_dispatcher` looks at
the focused entity, calls its bridge impl, and writes the tracker —
no crate updates the tracker directly. New global overlays
(jump-mode, which-key) extend `PaneMode` plus an `*_active` override
on the tracker; they never add a parallel indicator.

== Action registry

```rust
type ActionFn = fn(&mut Kernel, &ActionContext, ActionArgs) -> Result<()>;

pub struct Action {
    pub name: &'static str,
    pub function: ActionFn,
    pub accepts: &'static [ObjectKind],   // empty = nullary verb
}

pub struct ActionContext {
    session: SessionId,
    pane: Option<PaneId>,
    mode: Mode,
    selection: Selection,                  // pulled from focused pane
}
```

The `accepts` field is what makes object-verb work uniformly: each
verb declares which selection kinds it accepts, and the command
palette filters verbs by the focused pane's current selection type.
Section 6 covers this in detail.

Action names follow `codon_<area>::<Verb>` (matching the GPUI
`actions!` macro), e.g.:

```
codon_session::SessionNew    codon_session::WindowGoto(1)
codon_session::SessionSwitch codon_session::WindowOverview
codon_keymap::ShowKeymap     codon_jump::JumpToTarget
codon_panes::OpenAgent       codon_panes::PeekGit
codon_agent::Explain         codon_command_palette::Open
file_manager::OpenInPane     file_manager::Yank
```

== Keymap layering

The codon-keymap loader resolves bindings in four passes:

#set enum(numbering: "1.")

+ *Global* — works in every mode, every pane. Used for the chord
  prefix, session/window switching, command palette open.
+ *Per-pane-kind, per-mode* — `[bindings.editor.normal]`,
  `[bindings.terminal.insert]`, etc.
+ *Per-mode* — `[bindings.normal]`. Applies in any pane in that mode
  if no per-pane-kind binding shadows it.
+ *Embedded defaults* — Zed's vim+Helix bindings imported by the
  vendored editor; codon's defaults sit on top for cross-cutting
  verbs.

Default bindings live as an embedded TOML string in
`crates/codon-keymap/src/keymap.rs`. User overrides live in
`~/.config/codon/codon.toml` — the unified config file that also
holds the chord prefix, per-feature settings, and any TOML-reachable
knob (legacy `~/.config/codon/keymap.toml` is still read with a
deprecation hint).

The tmux-style chord prefix is configurable via
`[keymap] prefix = "<chord>"` (default `cmd-k`). The loader expands
the literal sentinel `prefix` in every keystroke string — defaults
*and* user bindings — to the resolved value at bind time. So
`"prefix s s"` in the embedded defaults binds as `"cmd-k s s"` out
of the box and as `"ctrl-x s s"` once the user sets
`prefix = "ctrl-x"`. The codon TOML is the *single source of
configuration* — codon never reaches into Zed's JSON keymap files
for cross-cutting verbs.

== Status and command line

A single bottom-row widget renders, depending on mode:

- *Normal/Insert:* a three-zone status bar (Phase 13). Left zone:
  global state (session, mode). Centre zone: pane context (path,
  language, position). Right zone: meta + dynamic messages
  (diagnostics counts, agent task indicator, transient toasts).
- *Command:* `:` prompt with fuzzy-completing input over the action
  registry.

Status content is sourced from kernel state via lightweight
observers. The mode indicator is themable and prominent enough to
read at a glance.

== Cheatsheet and which-key

Two discoverability surfaces, both keyboard-only (Phase 16):

- *Cheatsheet* (`codon_keymap::ShowKeymap`) — a modal showing every
  binding in the active pane's mode, grouped by namespace, with tabs
  for cross-pane verbs.
- *Which-key overlay* — when the chord prefix is pressed, a
  full-width panel auto-flips to the top of the window if it would
  occlude the focused pane, listing every key continuation. Replaces
  Zed's small bottom-right which-key panel.

== Pane host

Each window holds a layout tree; the pane host is the GPUI component
that renders it. Three node types:

- *Split* — two child panes side-by-side or stacked, with a
  keyboard-resizable separator (`cmd-k shift-{h,j,k,l}` via
  `vim::ResizePane*`).
- *Stack* — N panes in the same slot; currently degrades to "render
  active member" on apply. First-class stack rendering is deferred
  (see Section 7).
- *Leaf* — a single pane.

When the focused session or window changes, the host swaps trees via
`workspace::codon_bridge::{capture_layout, apply_layout,
replace_center_with_empty_pane, restore_center_root}`.

// ============================================================
= Selection-first interaction
// ============================================================

Helix's selection-first model — *the noun is selected and visible
before the verb fires* — generalizes beyond text. Every pane in
Codon has a typed object set; the same modal grammar applied to that
set yields a uniform UX where the same keys mean approximately the
same thing everywhere. This chapter spells out how, what's typed,
and which parts of the generalization land now versus later.

== The principle

Vim is verb-object: `d3w` commits to "delete" before "3 words" is
even visible. Helix is object-verb: select the 3 words, see them
highlighted, then press `d`. The user-facing property is *no
surprise* — the noun is inspectable before the verb fires, and not
pressing the verb undoes nothing.

This property is independent of text. It applies to any UI surface
where (a) there is a stable, typed set of selectable objects, and
(b) verbs operate on selections. Codon enforces it everywhere it
makes sense.

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
  [File manager],
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

*Terminal blocks.* Treating a command together with its output as
one typed object is the Warp/Wave insight. Once blocks exist, "select
the error block, send to agent, fix" replaces the find-copy-paste
loop. Block detection requires shell integration (OSC 133 prompt
markers) or heuristic boundary detection from the terminal stream —
solvable but not free.

*Agent objects.* Treating messages, tool calls, and code blocks in a
conversation as selectable objects turns the chat log from prose into
a navigable artifact. Re-run a tool call with edited arguments.
Branch the conversation from a chosen message. Apply a code block to
a buffer. All by selection plus verb.

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

This is the load-bearing structural decision: *selection lives at
the kernel boundary, not inside any single pane*. Without it,
cross-pane verbs are impossible and "selection-first" becomes
per-pane re-implementation.

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
Helix-shaped navigation model. The algebra is on the roadmap but not
shipped — Section 6.8 covers staging.

== Cross-pane verbs

Verbs that accept multiple object kinds unify workflows across panes:

```
codon_agent::Explain    accepts: [Text, Hunk, Block, Diagnostic, Message]
codon_agent::Fix        accepts: [Diagnostic, Block]   // block = error output
codon_agent::Refactor   accepts: [Text, File]
diff.against            accepts: [File, Commit, Branch]
git.blame.show          accepts: [Text, File]
fs.show                 accepts: [Path, FileMention]   // from agent output
```

Concrete workflows that drop out:

- *File manager:* select 12 files matching `*.ts`. Trigger
  `codon_agent::Explain` directly (no pane switch needed). The agent
  receives a `Files` selection and treats it as its working set.
- *Diagnostics panel:* select all `TS2345` errors across the project.
  Trigger `codon_agent::Fix`. The agent receives a `Diagnostics`
  selection and produces a single patch.
- *Git log:* select 3 commits. Trigger
  `codon_agent::WriteReleaseNotes`. The agent receives `Commits`.
- *Terminal:* select the block containing a stack trace.
  `codon_agent::Explain`. The block's command and output become the
  agent's context.

The user binds one key for `codon_agent::Explain` once and it works
in every applicable context. This is the primary payoff of
object-verb being a kernel-level concept rather than per-pane. The
`codon-agent` crate exists; the early cross-pane verbs (Explain /
Summarize / Refactor) ship in Phase 3; the full multi-kind catalog
is on the roadmap.

== Selection registers

Helix's registers (`"a`, `"b`, …) hold yanked text. Generalized:
any typed selection can be stored in a register. Registers persist
for the session; named registers (declared in config) persist
across sessions.

```
"f          "files I always review together"  → Files(...)
"e          last-error set                    → Diagnostics(...)
"c          cherry-pick candidates            → Commits(...)
```

Verbs that produce selections (`select-pattern`, `select-by-author`,
or the result of an agent query) can write into registers. Verbs
that consume selections can read from them: `"f open` opens all
files in register `f`; `"e fix` fixes all diagnostics in register
`e`.

Registers are the pipeline-composition mechanism: they make the
user's working set inspectable, named, and reusable instead of
fleeting. Registers as typed selections are on the radar — the
shipped Helix-mode text registers are the seed.

== Where the model doesn't fit

An honest list of cases where forcing object-verb is wrong:

- *Insert mode in editor and terminal.* No noun is being constructed;
  characters are being emitted. Insert mode is the escape hatch and
  isn't subject to the model.
- *Configuration verbs.* "Toggle word wrap," "increase font size."
  No object. These live in Command mode (`:`) as nullary verbs in
  the registry (`accepts: &[]`).
- *Verb arguments that aren't selection-shaped.* `:write some/path.rs`
  takes a string, not a noun in the grammar. Command mode has
  free-form arguments. We don't pretend everything is
  selection-shaped.
- *Continuous mouse-style operations.* Drag-resize, scrub a
  scrollbar. Awkward in object-verb. The few mouse-friendly
  affordances live outside the modal grammar — but codon's default
  is keyboard-only, so these are rare.

The principle: *selection-first wherever there's a stable typed
object set; Command mode for the rest.*

== Scope today and the path to full generality

What ships today (Phase 1):

- `ObjectKind`, `Selection`, `SelectionSource`, and
  `ActionAcceptsRegistry` interfaces defined in
  `REQ:codon/selection-first`. The command palette already filters
  actions by selection kind.
- Editor pane is the canonical `SelectionSource` implementation.
- `codon-agent` ships Explain / Summarize / Refactor seeded from
  selections; the agent panel's
  `AgentPanel::seed_explain_with_selection` is the entry point.

What's on the radar but not shipped:

- *Object-grammar trait* per pane kind (`next`, `inner_container`,
  `filter_by_predicate`). Each pane currently exposes its selection
  but not the refinement operators.
- *Full selection algebra* (`s` predicate filter syntax, `K`
  intersect, `A` append-cursor across panes). The shipped Helix
  algebra works on text; generalizing to other pane grammars is the
  open work.
- *Typed registers* persisted per-session and (named) across
  sessions. Helix's text registers ship; the typed-selection variant
  is the next step.
- *Cross-pane verb catalog* beyond the three in `codon-agent` today.

The architectural commitment is *not* deferring the foundations —
the interfaces are in the registry from Phase 1, even though most
verbs have empty or single-type accepts. The cost of doing this
correctly from day one was small; the cost of retrofitting it would
have been high.

// ============================================================
= Sessions, windows, and layout
// ============================================================

== Sessions

```rust
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub cwd: PathBuf,
    pub windows: Vec<Window>,
    pub active_window: usize,
    pub previous_window: Option<usize>,
    pub last_attached_ms: i64,
    // …
}

pub struct Window {
    pub id: WindowId,
    pub name: String,
    pub layout: Option<LayoutSnapshot>,
}
```

Sessions are tmux-style: each is a `(name, cwd, list-of-windows)`
tuple, persisted to the global KVP store under
`codon_sessions_v1`. There is no separate "is this a workspace"
concept; project context (LSP clients, git, diagnostics) accretes
lazily as features are used.

A new session is cheap: a name (defaulted to the project's primary
cwd), a cwd, one window holding one terminal pane.

Sessions are visible one at a time. Switching sessions swaps the
workspace's center pane group in place. Cross-session navigation
uses two surfaces:

- *Fuzzy switcher* (`codon_session::SessionSwitch`) — vendored
  picker over sessions by name + cwd + last-attached time.
- *Overview* (`codon_session::SessionOverview`) — tmux-style nested
  tree of sessions → windows. `j`/`k` move between rows, `h`/`l`
  collapse/expand, Enter attaches.

== Windows within a session

Each session contains an ordered list of windows. One is visible at
a time; the rest are reachable via keyboard. tmux verbs the user
expects all map to codon actions (Phase 11):

- `prefix 1`..`prefix 9` — direct jump to window N
  (`codon_session::WindowGoto`).
- `prefix l` — last-window toggle (`WindowLast`).
- `prefix ,` — rename window (`WindowRename`).
- `prefix !` — break the active pane into a new window
  (`BreakPaneToWindow`).
- `WindowOverview` mirrors `SessionOverview` but scoped to the
  active session.

The status bar shows a tab-bar-shaped indicator with one entry per
window, no close-X, click-to-switch (the only mouse affordance left
because the keyboard verbs cover everything).

== Layout snapshots

```rust
pub enum LayoutSnapshot {
    Group { axis, flexes, children },
    Stack { members, active },
    Pane(PaneSnapshot),
}
```

A serde-friendly mirror of Zed's center pane group. The codon-side
helpers live in `vendor/zed/crates/workspace/src/codon_bridge.rs`:
`capture_layout`, `apply_layout`,
`Workspace::replace_center_with_snapshot`. Item ids are preserved
across captures so editor buffers and terminal cwds rehydrate
cleanly.

Keyboard resize is bound to `cmd-k shift-{h,j,k,l}` via
`vim::ResizePane*`.

The `Stack` variant currently falls back to its active member when
applied — first-class `Member::Stack` rendering needs a refactor of
vendored Zed's `pane_group` (~15 match sites). Today's tabs cover
the common case; the stack refactor is queued as a nice-to-have for
nested layouts.

== Pane stash (in-memory)

Window-switching uses an *in-memory pane stash* alongside the
persisted snapshot. `WindowRuntimeCache` in
`crates/codon-session/src/runtime.rs` keeps cloned `Member` trees
plus active pane handles alive across switches, so panes (and their
workspace subscriptions) survive a window swap with no rehydrate
cost.

The persisted JSON `LayoutSnapshot` is the *fallback* for
cross-restart restoration only; in steady-state navigation, the
cache is authoritative.

== Persistence

State is written to the global KVP after every mutation via a
background spawn, plus a 30-second heartbeat. Per-pane handling on
rehydrate:

- *Terminal:* PTY does not survive process restart; the rehydrated
  pane shows the last scrollback and prompts to respawn.
- *Editor:* open file path and view state; rope content is re-read
  from disk; unsaved changes follow Zed's existing swap-file path.
- *File manager / git / diff / image:* enough state to recreate.
- *Agent:* full conversation history (Zed's agent crate already
  persists this; we use its mechanism).
- *Commit:* in-progress commit message text.

== Pane kinds via the panel adapter

Codon's modal multiplexer model assumes every surface is a pane in
the workspace tree. Zed ships seven views as dock-hosted `Panel`
impls — `AgentPanel`, `ProjectPanel`, `OutlinePanel`,
`TerminalPanel`, `GitPanel`, `DebugPanel`, `CollabPanel`. Per-panel
rewrites turned out to be uneconomic (two earlier attempts —
agent and git status — were deferred / wontdo'd).

Phase 12's `codon-panes` crate solves this with a single
`PanelItemAdapter<P: Panel>` that wraps any Zed `Panel` impl as a
`workspace::Item`. Verdicts:

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 7pt,
  align: (left, left),
  table.header([*Panel*], [*Codon verdict*]),
  [`AgentPanel`], [convert — host via adapter as a regular pane],
  [`GitPanel`], [convert],
  [`OutlinePanel`], [convert],
  [`DebugPanel`], [convert],
  [`ProjectPanel`], [already-replaced by `file-manager`],
  [`TerminalPanel`], [drop — terminals are panes already; the
   *Panel* was just the dock host],
  [`CollabPanel`], [drop — single-user fork],
)

An opt-in *peek* placement remains as an escape hatch: a single
reusable dock surface mounts the same panel view transiently,
auto-dismisses on focus-loss or `esc`, never persists across windows
or restarts. Peek is off by default; each convertible panel
declares a preferred side and a peek keybinding separate from its
open-as-pane keybinding.

// ============================================================
= Implementation plan
// ============================================================

Sequential phases, each ending in a usable artifact. Phases 1–13
have shipped; 14–17 are in flight or planned.

== Shipped

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 7pt,
  align: (left, left),
  table.header([*Phase*], [*Outcome*]),
  [1], [Modal shell & action layer: `PaneMode`,
   `CodonModeTracker`, TOML keymap, selection-first interfaces,
   yazi-style file manager.],
  [2], [Tmux-style sessions and windows, `LayoutSnapshot`,
   keyboard pane nav/resize, persistence to KVP.],
  [3], [Agent pane and cross-pane verbs (Explain / Summarize /
   Refactor), inline assistant, AI-generated commit messages.
   Per-panel "convert agent to pane" attempt deferred — superseded
   by Phase 12.],
  [4], [Git integration (panel + diff pane) and unified TOML
   config. Buffer-trait sub-goal *superseded* — codon adopted
   Zed's vim + `helix_default` wholesale.],
  [5], [Native UX coverage: file-manager polish + additional pane
   types (diff, image, diagnostics).],
  [6–8], [File-manager parity with yazi in three waves: nav
   extras + sort/display + visual range (6); find / search /
   openers / shell exec (7); symlinks, bulk-edit, tasks,
   trash (8).],
  [9], [Editor jumps + file-manager visual polish.],
  [10], [Vimium-style global hint-mode jumps
   (`codon_jump::JumpToTarget`).],
  [11], [Window-nav tmux parity: last-window toggle, direct index,
   2-key motion, rename, safe-close, break-pane.],
  [12], [Panes-from-panels — one `PanelItemAdapter` over every Zed
   `Panel` impl, with opt-in peek as escape hatch.],
  [13], [Status-bar overhaul: three-zone modeline with prominent
   mode + window indicators.],
)

== In flight

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 7pt,
  align: (left, left),
  table.header([*Phase*], [*Scope*]),
  [14], [Codebase hygiene & consistency pass. Eliminate `unwrap()`
   and silent `let _ =` in production code, extract a shared modal
   scaffold (`codon-pickers`), unify `CodonModeTracker` through
   `PaneModeBridge`, decouple `codon-keymap` from downstream
   crates, collapse the dual registry in `workspace::codon_bridge`,
   raise the test floor in untested codon crates, reconcile doc
   drift in CLAUDE.md and this design doc.],
  [15], [Tmux-prefix parity: configurable chord prefix via
   `[keymap] prefix = "<chord>"`, double-prefix passthrough to the
   focused terminal, `MovePaneToWindow(usize)` for
   `prefix shift-<N>`.],
  [16], [Helix UX coverage: duplicate vim+Helix bindings into the
   codon TOML defaults (cheatsheet visibility), replace Zed's
   small which-key panel with a full-width codon overlay,
   Helix-style pickers (file / buffer / symbols / diagnostics /
   jumplist / changed-files / last-picker), wire Helix shell
   verbs (`|` `!` `$` `Alt-|` `Alt-!`) and a matching `:sh` /
   `:pipe` palette path.],
  [17], [App-wide performance: custom Elements for the
   file-manager view (row + column + glyph caches + deferred
   editor preview + dirty-rect repaint + measurement harness),
   plus session/window switch optimizations
   (skip-capture-on-cache-hit, defer persist, fix O(N×M) pane-set
   merge in `restore_center_root`, elide unconditional
   pane-notify, defer overview-modal capture).],
)

// ============================================================
= Open questions and decisions deferred
// ============================================================

== Confirmed

- Single process, single binary. No daemon, no protocol, no remote.
- Project name: *Codon*.
- Editing model: *Zed's vim mode with `helix_default` force-enabled*.
  Helix is the reference experience; no helix-editor crates are
  vendored as a runtime.
- Terminal backend: *alacritty_terminal*, as Zed uses today.
- Reuse: maximum of Zed's product features.
- Layout: Split / Stack / Leaf. No buffer tabs; tabs-as-windows
  cover the common case; first-class stacks deferred.
- Default pane kind: terminal.
- Display model: one session shown at a time, one window within
  that session.
- *Selection-first / object-verb across panes.* Interfaces shipped
  in Phase 1 (`Selection`, `ObjectKind`, `SelectionSource`,
  `ActionAcceptsRegistry`, `Action::accepts`); full object grammar,
  selection algebra, and typed registers are on the roadmap.
  See Section 6.
- *Configuration:* single unified
  `~/.config/codon/codon.toml`. TOML is the only source of
  per-feature settings, including the chord prefix. Codon never
  routes cross-cutting verbs through Zed's JSON keymap.
- *Keyboard-first.* Mouse-only affordances from Zed are stripped
  whenever a keybinding covers the same action.
- Multi-OS-window, WASM plugins, predictive echo: deferred.

== Cleanup pending

- *`crates/codon-buffer/` removal.* Zero consumers; superseded by
  the decision to keep `language::Buffer` directly. Tracked by
  `TASK:phase-14/codon-buffer-removal`.
- *Doc drift.* `TASK:phase-14/doc-drift-resolve` reconciles
  CLAUDE.md and this design doc with the shipped phases.

== Deferred to v0.next (with notes)

The architecture is intentionally extension-friendly. When (if) we
pick these up, here is the rough cost.

#table(
  columns: (auto, 1fr),
  stroke: (x: none, y: 0.4pt + luma(180)),
  inset: 8pt,
  align: (left, left),
  table.header([*Deferred*], [*Approximate cost when picked up*]),
  [First-class stacked panes],
  [Refactor vendored Zed's `pane_group` to add a `Member::Stack`
   variant (~15 match-site impacts). Add a no-close-X tab strip
   for stack rows and `pane.stack.cycle/add/remove` actions. The
   `LayoutSnapshot::Stack` round-trip already exists; the gap is
   live rendering.],
  [Daemon split],
  [Extract codon-kernel crates plus the state-owning halves of
   pane crates into a `codon-daemon` binary. Define view protocol
   schemas. The `codon-*` crate boundaries are the natural seam.],
  [Capnp protocol formalization],
  [Comes with the daemon split. Two schema sets; one channel
   multiplexer over a Unix socket.],
  [Remote sessions over SSH],
  [Build `agentd` (~4 kLOC). Capability protocol over SSH
   ControlMaster. Per-session capability binding (local OR
   remote).],
  [Mosh-style transport],
  [UDP-based transport with state replay. Predictive echo for
   editor and terminal panes.],
  [WASM plugin runtime],
  [A `plugin` capability and an in-app sandboxed WASM host.
   Plugins can register actions, define pane kinds, or proxy
   capabilities.],
)

== Out of scope

- Cross-platform priority. Linux first; macOS likely; Windows is
  not a goal and may never be one.
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
  [*Action*], [A named operation in the registry; invoked by
   keymap or command palette. Carries an `accepts: &[ObjectKind]`
   declaration. Named `codon_<area>::<Verb>` for new actions.],
  [*CodonModeTracker*], [Global, focused-pane-driven reflection of
   the current `PaneMode`. Updated only via the
   `install_pane_mode_dispatcher` focus subscriber; never written
   directly.],
  [*Object grammar*], [A pane kind's declaration of its object
   types and the refinement operators (`next`, `inner_container`,
   `filter_by_predicate`, …) over them. Roadmap item; trait
   sketched in Section 6.],
  [*ObjectKind*], [The discriminant of `Selection` — Text, File,
   Hunk, Commit, Block, Message, Diagnostic, etc. Used by
   `Action::accepts` to gate verb applicability.],
  [*Pane*], [A typed leaf in a window's layout; one of terminal,
   editor, file manager, diff, git pane, agent pane, image, commit,
   debug, outline.],
  [*PaneMode*], [Per-pane `Normal | Insert | Command`. Hosts global
   transient overlays (jump-mode, which-key) via `*_active`
   overrides on the tracker.],
  [*Pane host*], [The GPUI component that recursively renders a
   `LayoutNode` for the active window.],
  [*PanelItemAdapter*], [Single `codon-panes` wrapper that turns
   any Zed `Panel` impl into a workspace `Item`. Replaces seven
   per-panel forks.],
  [*Register*], [A named slot holding a typed `Selection`.
   Per-session by default; named registers persist across
   sessions. Typed-selection registers are roadmap; Helix text
   registers ship today.],
  [*Selection*], [A typed value representing the currently-targeted
   nouns in the focused pane. Pulled into action context by the
   kernel via `SelectionSource`.],
  [*Session*], [A unit of context: name, cwd, and an ordered list
   of windows. tmux-style; one OS window shows one session at a
   time.],
  [*Stack*], [A layout node holding N panes in the same slot, one
   visible at a time. Currently degrades to active-member-only on
   apply; live rendering deferred. Today's "tabs" surface is the
   window-bar, not a stack.],
  [*Vendored*], [Forked into Codon's monorepo under `vendor/`. We
   modify freely; we rebase from upstream on our schedule.],
  [*Window*], [A pane-layout container inside a session. tmux
   semantics — switching windows is a layout swap with no cwd
   change.],
)
