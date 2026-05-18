---
id: REQ:codon/keymap
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Codon's keymap surface — TOML loader, embedded defaults, and the
  tmux-style chord prefix. The prefix MUST be a user setting (no
  hard-coded `cmd-k` in defaults), double-tap MUST send the
  literal prefix through to the focused terminal, and codon MUST
  expose an action to move the active pane into an existing window
  by index.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-15]
---

# Codon keymap — configurable prefix, passthrough, pane→window

## Context

The codon keymap surface today (`crates/codon-keymap/src/keymap.rs`)
hard-codes `cmd-k` as the tmux-style chord prefix throughout the
embedded `DEFAULT_KEYMAP` string. Users coming from a tmux config
keyed on `ctrl-x` (or `ctrl-b`, or `ctrl-a`) have to either
re-add ~40 mirror bindings in their own `codon.toml`
([see the existing user-config shape in
`assets/config/codon.example.toml`](spec:src:assets/config/codon.example.toml))
or live with two parallel prefixes. The user config can only *add*
bindings, never unbind defaults, so the `cmd-k` chords stay active
even after a mirror block is added — both prefixes fire, neither
matches the user's tmux muscle memory exclusively.

Two tmux conveniences are also absent:

- **Double-prefix passthrough.** In tmux, `prefix prefix` sends the
  literal prefix keystroke to the focused program. Codon's chord
  engine ([GPUI keymap dispatcher,
  `gpui::set_keystroke_chord_timeout`](spec:src:vendor/zed/crates/gpui/src/keymap/matcher.rs))
  swallows the second keystroke without a passthrough hook.
- **Move pane to existing window.** tmux's `join-pane -t :N` moves
  the active pane into window N. Codon has
  [`codon_session::BreakPaneToWindow`](spec:src:crates/codon-session/src/break_pane.rs)
  (split into a *new* window) but no equivalent for an existing
  index — natural binding `cmd-shift-<N>` is therefore unwired.

:::{requirement id="keymap" level="MUST"}
The system MUST provide:

- {#c-prefix-configurable} a single user-facing setting that
  selects the chord prefix used by the embedded defaults
  (`[keymap] prefix = "ctrl-x"` in `~/.config/codon/codon.toml`,
  default `"cmd-k"` for backward compatibility). `DEFAULT_KEYMAP`
  MUST stop hard-coding `cmd-k` — chords that today read
  `"cmd-k X"` MUST read `"prefix X"`, and the loader expands the
  literal token `prefix` to the configured chord at bind time.
  User overrides in `[bindings.*]` MAY use the same `"prefix X"`
  shorthand and MUST expand identically. Switching the prefix MUST
  take effect on the next keymap reload (no restart) and MUST NOT
  leak bindings under the previous prefix into the active set.

- {#c-prefix-passthrough} a chord pattern that lets the user send
  the configured prefix keystroke through to a focused terminal
  pane by tapping it twice (tmux `send-prefix`). The
  `prefix prefix` chord MUST be bound by default to a
  passthrough action that forwards the literal prefix keystroke to
  the focused terminal's PTY when the active pane is a terminal,
  and MUST be a silent no-op elsewhere. Implementation may extend
  the GPUI chord matcher or wire a codon-side terminal action — the
  contract is observable behavior, not the mechanism.

- {#c-move-pane-to-window} a `codon_session::MovePaneToWindow(usize)`
  action that moves the active pane into an existing window by
  zero-based index within the active session, preserving the pane's
  items and focus state (mirrors tmux `join-pane -t :N`). The
  embedded defaults MUST bind `prefix shift-<N>` for `N=1..9` to
  `MovePaneToWindow(N-1)`. Out-of-range indices MUST be silent
  no-ops with a toast notification. Moving the *only* pane in the
  source window MUST close that window (consistent with
  `BreakPaneToWindow`'s single-pane handling).
:::

## Approach

The prefix-configurable clause is a small loader change: extend
`CodonKeymap` with an optional `[keymap]` table, resolve the prefix
during `load_codon_keymap` (defaults → user override), then walk
every parsed chord and substitute a leading `"prefix"` token before
calling `KeyBinding::load`. Tests cover both the substitution rule
and the round-trip from a custom-prefix `codon.toml` to the bound
chord set.

The passthrough clause is the trickiest of the three because GPUI's
matcher resolves chords greedily. The likely path is a small
extension to
[`gpui::Keystroke` / matcher state](spec:src:vendor/zed/crates/gpui/src/keymap/matcher.rs)
that recognizes a configurable "self-insert sentinel" action and
falls through to the focused element's keystroke handler with the
original keystroke restored. Spec the surface area before writing
code — this touches vendored Zed.

The move-pane-to-window clause reuses the snapshot surgery in
[`crates/codon-session/src/break_pane.rs`](spec:src:crates/codon-session/src/break_pane.rs):
the existing helper detaches a pane subtree from one window's
`Member` tree; a sibling helper attaches that subtree to another
window's `Member` tree as a new horizontal split. Action shape
mirrors `WindowGoto(usize)` (newtype tuple struct, deserialised
from `(N)` in TOML).
