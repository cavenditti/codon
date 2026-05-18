---
id: TASK:phase-15/keymap-prefix-passthrough
type: task
status: draft
version: 0.0.1
summary: >
  Bind `prefix prefix` to a passthrough action that forwards the
  literal prefix keystroke to the focused terminal pane's PTY
  (tmux `send-prefix`). Silent no-op outside terminals.
owners: [carlo]
progress: done
refines:
  - REQ:codon/keymap#c-prefix-passthrough
---

# Double-prefix passthrough to terminal

## What changes

Tmux's `send-prefix` sends the literal prefix keystroke to the
focused program — essential when the program (vim, emacs, an inner
tmux) also wants the chord. Codon needs the same.

The chord engine in GPUI
([`vendor/zed/crates/gpui/src/keymap/matcher.rs`](spec:src:vendor/zed/crates/gpui/src/keymap/matcher.rs))
greedily resolves chords. A naive binding of `prefix prefix` would
fire an action whose handler manually writes the keystroke into the
focused terminal — but reconstructing the raw bytes from a
`gpui::Keystroke` is fragile (modifier-aware keymap, OS-specific
deadkeys, …) and the terminal's existing keystroke pipeline is
already correct. Better path: dispatch a
`codon_keymap::SendPrefixToFocus` action whose handler resolves
the configured prefix string, calls
[`gpui::Keystroke::parse`](spec:src:vendor/zed/crates/gpui/src/keymap/keystroke.rs)
on it, and dispatches a synthetic key event back through the
focused element's keystroke chain (terminal `TerminalView` knows
how to translate a `Keystroke` into PTY bytes via Alacritty's
existing converter).

Implementation outline:

- New `actions!(codon_keymap, [SendPrefixToFocus])`.
- Handler in `codon-keymap` (or `codon-session` if that's the more
  natural home): look up the live prefix via a shared accessor
  (`codon_keymap::resolved_prefix() -> &'static str` — needs a
  process-global written by the loader; same `OnceLock` pattern as
  `gpui::set_keystroke_chord_timeout`); parse the prefix into a
  `Keystroke`; check the focused entity is a terminal pane (via
  `pane_mode == Normal && Terminal` predicate or by downcasting
  the active item); dispatch the keystroke through
  [`Window::dispatch_keystroke`](spec:src:vendor/zed/crates/gpui/src/window.rs)
  or the equivalent.
- DEFAULT_KEYMAP gains `"prefix prefix" = "codon_keymap::SendPrefixToFocus"`.
- Outside terminals: log at `trace!` and return — no toast (the
  user explicitly asked for "passthrough or no-op", not "warn me").

Update sites:

- `crates/codon-keymap/src/keymap.rs` — store the resolved prefix
  in a process-global accessor.
- `crates/codon-keymap/src/passthrough.rs` (new) — the action and
  handler.
- `apps/codon/src/main.rs` — call `codon_keymap::register_passthrough(cx)`
  from the init chain.
- `crates/codon-keymap/src/codon_keymap.rs` — re-export
  `SendPrefixToFocus` and `register_passthrough`.

Tests:

- `passthrough_dispatches_in_terminal` — focus a terminal, fire
  `SendPrefixToFocus`, assert the PTY received the prefix bytes.
  May require a fake terminal harness; if too heavy, replace with
  a unit test on the keystroke-resolution helper plus a manual
  smoke test in the Done-when checklist.
- `passthrough_noop_in_editor` — focus an editor, fire the action,
  assert nothing changes (no error, no inserted text).
- `passthrough_handles_chord_prefix` — when the resolved prefix is
  a multi-key chord (theoretically possible — `cmd-k` is itself
  a single keystroke, but a user could set `prefix = "ctrl-x ctrl-y"`),
  document the chosen behavior. Reasonable default: refuse with a
  warning at load time; accept only single-keystroke prefixes.
  Encode that constraint in the prefix-configurable task too.

## Why this clause

A user with vim or an inner tmux inside a codon terminal pane will
hit chord prefixes that the inner program also wants. Without
passthrough they can never reach those keystrokes. Tmux solves
this with `send-prefix`; codon should match.

## Done when

- `prefix prefix` sends the resolved prefix keystroke to a focused
  terminal pane, and the focused shell sees the literal byte
  sequence.
- Outside a terminal the action is a silent no-op.
- A single-keystroke constraint on the prefix value is enforced at
  load time (warn + ignore override otherwise; fall back to the
  default).
- Tests above pass; manual smoke recorded in the commit body.
- `spec lint` reports zero errors.
- `vendor/zed/script/clippy` reports no new warnings (the
  vendored-Zed touch, if any, follows upstream conventions).
