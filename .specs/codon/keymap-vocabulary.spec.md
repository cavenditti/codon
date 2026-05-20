---
id: REQ:codon/keymap-vocabulary
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Shrink the codon chord vocabulary the user has to memorise: collapse
  verbs that fragment by pane kind into single context-aware actions,
  rename anti-mnemonic chords (`prefix w` becomes the window-verbs
  sub-prefix, `prefix shift-w` the single-chord overview, `prefix l`
  drops, `prefix shift-t/e` become always-new), and route pickers
  through a global `space`-leader flow rather than the overloaded
  `prefix p X` sub-prefix.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-20]
---

# Codon keymap vocabulary

## Context

Today's embedded defaults
([`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs))
expose ~120 bound chords across six pane kinds. Two structural issues
make the surface harder to memorise than its size warrants:

1. **Pane-kind verb fragmentation.** Several verbs split into multiple
   actions purely because the target pane kind differs:
   - `codon_session::SplitTerminal{Right,Down}` /
     `…SplitFileManager{Right,Down}` / `pane::SplitRight` /
     `pane::SplitDown` — four codon-side actions plus two raw Zed ones.
   - `prefix t` / `prefix e` are bound to raw `workspace::NewTerminal`
     / `file_manager::Open`, but `cmd-t` / `cmd-e` are bound to
     `codon_session::GotoOrOpen{Terminal,FileManager}`. Two flavours
     of "open a terminal" live side by side under similar chords.
   - `codon_session::SafeCloseActiveItem` (the smart cascade:
     item → pane → window → empty) coexists with raw
     `pane::CloseActiveItem` (helix `space w q`), which bypasses the
     cascade.

2. **Anti-mnemonic chords.** A handful of bindings actively fight muscle
   memory:
   - `prefix w` binds `codon_session::SafeCloseActiveItem` (Mac
     `cmd-w` convention) while `prefix shift-w` is the window-verbs
     sub-prefix. A user with tmux muscle memory expects `prefix w`
     to open the window list and will close a pane by accident.
   - `prefix l` binds `codon_session::WindowLast`; `ctrl-l` binds
     `workspace::ActivatePaneRight`. Same letter, two unrelated
     verbs depending on the prefix.
   - `prefix p` is overloaded as both the bare-leaf `WindowPrev`
     binding *and* the sub-prefix for every Helix-style picker
     (`prefix p f`, `prefix p b`, `prefix p s`, `prefix p g`,
     `prefix p j`, …). The 2-key leaf only fires on chord timeout;
     the cheatsheet shows both shapes.

Phase 20 fixes the vocabulary in one consolidated pass so the rest
of the discoverability work
([REQ:codon/discoverability](spec:REQ:codon/discoverability)) builds
on a stable surface. The fix is intentionally a breaking change to
the embedded defaults — users with overrides in
`~/.config/codon/codon.toml` keep their bindings; the example config
ships with the new defaults; the changelog flags the renames.

:::{requirement id="keymap-vocabulary" level="MUST"}
The system MUST provide:

- {#c-verb-collapse-split} a single pair of `codon::Split{Right,Down}`
  actions (or equivalently-named `codon_session::Split{Right,Down}`)
  that pick the new pane's kind from the active pane's focus —
  terminal-focused → new terminal, file-manager-focused → new
  file-manager, editor-focused → new editor, seeding cwd from the
  active pane's path where applicable. The existing chord shapes
  MUST keep their visual intent: `prefix \` and `prefix |` retain
  the right-split mnemonic (`\` opens whatever the active pane
  kind is by default, `|` flips the kind — terminal ↔ file-manager
  — to mirror today's `Split{Terminal,FileManager}{Right,Down}`
  pairing); `prefix -` / `prefix _` mirror for down-split. The four
  pane-kind-specific actions MAY be retained as private dispatch
  targets but MUST NOT appear in the embedded defaults' chord table.

- {#c-verb-collapse-open-or-focus} `prefix t` / `prefix e` MUST bind
  `codon_session::GotoOrOpenTerminal` / `…GotoOrOpenFileManager`,
  matching the existing `cmd-t` / `cmd-e` semantics. The "always
  open a new instance" variants MUST live on `prefix shift-t` /
  `prefix shift-e` (new chords introduced in this phase), backed
  by raw `workspace::NewTerminal` and a sibling
  `file_manager::OpenNew` (or equivalent) that skips the existing-
  instance lookup.

- {#c-verb-collapse-close} `codon::Close` (the renamed
  `SafeCloseActiveItem` cascade) MUST be the single user-facing
  close verb. `pane::CloseActiveItem` MUST be removed from the
  Helix mirror block in the embedded defaults (`space w q` rebinds
  to `codon::Close`). A `codon::CloseForce` action MAY be added
  for the rare case where the cascade is undesired; if added it
  MUST NOT be bound by default.

- {#c-chord-window-prefix} the window verb family MUST live under
  `prefix w` as its sub-prefix (was `prefix shift-w`). Specifically:
  `prefix w n` = WindowNew, `prefix w h` = WindowPrev,
  `prefix w l` = WindowNext, `prefix w shift-l` = WindowLast,
  `prefix w c` = WindowClose, `prefix w w` = WindowSwitch,
  `prefix w r` = WindowRename, `prefix w !` = BreakPaneToWindow.
  `prefix shift-w` (single chord, no continuation) MUST bind
  `codon_session::WindowOverview`. The previous `prefix w`
  binding to SafeCloseActiveItem MUST be removed — `cmd-w` remains
  the close chord (Mac convention) and no `prefix`-based close
  chord is provided by default.

- {#c-chord-window-nav-leaves} the bare-leaf window-navigation
  chords MUST remain on `prefix n` (WindowNext) and `prefix p`
  (WindowPrev) — they keep the tmux muscle-memory path independent
  of the `prefix w …` discoverable menu. `prefix l` (today's
  WindowLast leaf) MUST be dropped from the embedded defaults;
  WindowLast remains reachable via `prefix w shift-l`.

- {#c-leader-pickers} pickers MUST be reachable via a global
  `space`-leader flow that fires in *every* pane kind whose Normal
  mode is published (editor / terminal / file_manager / git_panel
  / agent / outline / debug — anywhere a `pane_mode == normal` or
  `vim_mode == normal|helix_normal|helix_select` predicate already
  exists). The picker set MUST cover at minimum:
  `space f` = file finder, `space b` = tab switcher,
  `space s` = outline, `space shift-s` = project symbols,
  `space d` = diagnostics, `space r` = recent projects,
  `space g` = changed-files picker,
  `space j` = jumplist picker, `space '` = last picker.
  The previous `prefix p X` chain MUST be removed from the embedded
  defaults (the bare-leaf `prefix p` WindowPrev binding stays per
  `c-chord-window-nav-leaves`). Where a pane Normal mode already
  uses `space` as a leader (the editor Helix mirror block), the
  global flow MUST converge with it — same letter, same picker,
  same action — and MUST NOT shadow existing `space <letter>`
  Helix verbs that aren't picker openers.

- {#c-fm-hidden-rebind} the file-manager toggle-hidden binding
  MUST move from `.` to `, h` (joining the existing `,` view-
  options sub-prefix that already hosts the sort chords). The
  bare `.` chord MUST be freed in every Normal-mode context for
  the action-history repeat in
  [REQ:codon/discoverability#c-action-history-ring](spec:REQ:codon/discoverability#c-action-history-ring).
:::

## Approach

The verb-collapse clauses (split / open-or-focus / close) are mostly
TOML rewrites in `DEFAULT_KEYMAP` plus a thin dispatcher in
`codon-session` that picks the active pane's kind. The dispatcher
can reuse the existing logic in
[`codon_session::GotoOrOpen*`](spec:src:crates/codon-session/src/goto_or_open.rs)
and the cwd-seeding paths in
[`codon_session::SplitTerminal*`](spec:src:crates/codon-session/src/split.rs).

The chord-rename clauses (`prefix w` / `prefix shift-w` swap,
`prefix l` drop) are pure TOML edits — every chord under the old
`prefix shift-w …` family moves to `prefix w …`, and the embedded
`prefix w` close binding is deleted. The example config
([`assets/config/codon.example.toml`](spec:src:assets/config/codon.example.toml))
is updated in lockstep so new installs see the new chords; the
changelog notes the breaking change for existing users.

The `space`-leader picker flow is the only non-trivial structural
piece. Today only the editor Helix block uses `space` as a leader,
and crucially the codon TOML loader compiles `[bindings.editor.normal]`
under a `vim_mode == normal | helix_normal | helix_select` predicate.
For `space` to work as a global leader, codon needs a fresh predicate
that fires across all pane kinds when their Normal mode is active —
either by extending the loader to emit a new `[bindings.global.normal]`
section that compiles to
`Editor && (vim_mode == normal|helix_normal|helix_select)
  || ((Terminal || FileManager || GitPanel || …) && pane_mode == normal)`,
or by adding the same chord to every `[bindings.<pane>.normal]` block.
The spec leaves the mechanism open; the TASK pins it down.

The FM `.` → `, h` move is a single-line TOML edit plus a paired
removal from
[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
key-handler if `.` is hard-coded there outside the TOML path.

## Out of scope

- The user's `~/.config/codon/codon.toml` overrides are not
  touched. Users who explicitly rebound the affected chords keep
  their bindings; the changelog flags the embedded defaults
  change.
- Mnemonic audit beyond the four chords called out above. Other
  marginal cases (`prefix !`, `prefix r`) stay as-is until a user
  signal motivates the move.
- Verb collapse beyond split / open-or-focus / close. Other
  pane-kind-fragmented verbs (e.g. agent panel verbs) stay on
  their existing shapes.
