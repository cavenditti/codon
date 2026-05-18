---
id: REQ:codon/which-key-overlay
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Replace the small bottom-right floating panel from vendored Zed's
  `which_key` crate with a codon overlay that sits across the full
  width of the active pane at the bottom edge, auto-flips to the
  top edge when its content height would occlude more than a
  configurable fraction of the pane, and multi-columns its rows so
  it scales to large chord families without scrolling.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-16]
---

# Helix-style which-key chord overlay

## Context

Vendored Zed ships a which-key implementation at
[`vendor/zed/crates/which_key/`](spec:src:vendor/zed/crates/which_key/which_key_modal.rs).
It hooks `window.observe_pending_input` to detect a held chord
prefix, then renders a `ModalView` with the matching possible
bindings. The renderer (`which_key_modal.rs:238-256`) positions the
panel as `absolute().bottom(bottom_offset).right(px(8.))` with
`max_w = min(viewport_width * 0.5, 480px)` and
`max_h = viewport_height * 0.4`. The result is a small column-stacked
list pinned to the bottom-right corner of the OS window.

Helix's terminal HUD differs:

1. It spans the **full width** of the terminal.
2. It sits at the bottom **of the active editor area**, not the
   whole window — codon's analog is the active pane's bounds.
3. It multi-columns its rows so a wide chord family (`g …`,
   `space …`) fits without scrolling.
4. When the active pane is too short for the HUD to fit at the
   bottom without eating the pane content, the HUD flips to the top
   edge of the pane.

Codon already wraps several vendored panels with codon-side
overlays (the chord cheatsheet, the resize-sticky overlay,
codon-jump). The same shape applies here: a new `codon-which-key`
crate consumes the same `observe_pending_input` /
`possible_bindings_for_input` gpui APIs, registers its own
`ModalView`, and gets installed by `apps/codon/src/main.rs`.
Zed's `which_key::init` is not called from codon's init path, so
the two implementations never compete.

:::{requirement id="which-key-overlay" level="MUST"}
The chord HUD MUST:

- {#c-full-pane-width} span the full width of the active pane's
  bounds (not the OS window, not a fixed pixel max). Read via
  `Workspace::active_pane()` + `window.viewport_size()`.
- {#c-bottom-default} render anchored to the bottom edge of the
  active pane by default, with the same status-bar clearance the
  vendored implementation already computes.
- {#c-auto-flip} flip to the top edge of the active pane when its
  natural content height would exceed
  `pane.bounds.size.height * threshold`, where `threshold` is
  configurable via `[which_key] flip_threshold = <float>` in
  `~/.config/codon/codon.toml` (default `0.5`).
- {#c-multi-column} lay rows out in `N` columns where `N` is
  computed from `pane.bounds.size.width / min_column_width`
  (`min_column_width` configurable, default ≈ 240 px). Single
  column when the pane is narrow.
- {#c-group-rendering} preserve the vendored implementation's
  "group bindings by first remaining keystroke" grouping
  (`which_key_modal.rs::group_bindings`) — codon's overlay should
  reuse the same grouping logic rather than reinvent it.
- {#c-suppress-zed} install codon's overlay in place of Zed's:
  `apps/codon/src/main.rs` MUST NOT call `which_key::init(cx)` once
  the codon overlay is wired.
- {#c-settings-flag} let users disable the overlay entirely via
  `[which_key] enabled = false` in `codon.toml`, mirroring the
  existing `WhichKeySettings.enabled` flag.
- {#c-respect-delay} respect a `[which_key] delay_ms` setting (same
  semantics as the vendored implementation — wait N ms after the
  prefix before showing) so users on fast chords don't see a flash.
- {#c-dismiss-on-input-clear} dismiss when
  `window.pending_input_keystrokes()` returns `None`, exactly as
  the vendored implementation does today.
- {#c-mode-aware-title} surface the current `CodonModeTracker`
  pane-mode (Normal / Insert / Command / Select) as part of the
  title line, so users can confirm the chord context at a glance.
:::

## Why this REQ

The chord-prefix family is codon's biggest UX surface (≈40 chords
across sessions, windows, panes, agent, git, peek, jumps, palette,
cheatsheet). The vendored which-key works but reads as an
afterthought — small, off in the corner, single-column, frequently
covered by the active pane. Codon owns the multiplexer chrome and
should own the chord HUD too. The auto-flip rule is the only piece
that actually matters when the active pane is short (which it often
is in tiled layouts), and the multi-column rendering is what makes
the HUD usable for the `g …` and `space …` families that have 15+
chords.

## Done when

- A `codon-which-key` crate exists at
  `crates/codon-which-key/` with a `codon_which_key.rs` lib root
  per codon naming conventions.
- `apps/codon/src/main.rs` calls `codon_which_key::init(cx)` and
  does not call the vendored `which_key::init`.
- The overlay renders full-width across the active pane, flips
  when the threshold is hit, and multi-columns its rows.
- `[which_key]` settings are documented in
  `assets/config/codon.example.toml`.
- `spec lint` is at zero errors.
- `cargo clippy -p codon-which-key` reports no warnings.
