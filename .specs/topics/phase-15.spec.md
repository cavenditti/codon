---
id: TOPIC:topics/phase-15
type: topic
status: draft
version: 0.0.1
summary: >
  Configurable tmux-style chord prefix, double-prefix passthrough
  to the focused terminal, and a `MovePaneToWindow(usize)` action
  binding `prefix shift-<N>` to send the active pane into an
  existing window by index — the three pieces the user-config
  layer cannot reach.
owners: [carlo]
---

# Phase 15 — Tmux-prefix parity

Phase 1–14 grew codon into a tmux-style multiplexer with sessions,
windows, panes, and a tmux-shaped chord prefix family (`cmd-k c`,
`cmd-k n`, `cmd-k shift-w !`, …). The prefix itself was hard-coded
to `cmd-k` because the codon owner's muscle memory used cmd as the
modifier — but the broader tmux community keys off `ctrl-x`,
`ctrl-b`, `ctrl-a`, etc., and the user-config TOML can only *add*
bindings, never unbind defaults. The result: anyone who wants
something other than `cmd-k` ends up with two live prefixes.

Phase 15 closes the gap with three changes the user-config layer
cannot reach:

- **Configurable prefix.** Move `cmd-k` out of `DEFAULT_KEYMAP` and
  behind a `[keymap] prefix = "..."` setting in
  `~/.config/codon/codon.toml`. The loader substitutes a literal
  `"prefix"` token before binding, so every chord family
  (`prefix s s`, `prefix shift-w n`, …) re-keys atomically. Default
  stays `cmd-k` for backward compatibility.

- **Double-prefix passthrough.** `prefix prefix` sends the
  literal prefix keystroke through to the focused terminal pane
  (tmux `send-prefix`). Requires a small extension to the GPUI
  chord matcher in vendored Zed.

- **Move pane to existing window.** `codon_session::MovePaneToWindow(usize)`
  reuses `break_pane.rs`'s snapshot surgery to detach a pane from
  one window's `Member` tree and attach it to another's. Bound by
  default to `prefix shift-<N>` for `N=1..9`.

The three changes refine
[`REQ:codon/keymap`](spec:.specs/codon/keymap.spec.md) clauses
`c-prefix-configurable`, `c-prefix-passthrough`, and
`c-move-pane-to-window` respectively. Phase 15 ships when all
three TASKs are `done` and `spec lint` is at zero errors.
