---
id: TASK:phase-16/which-key-overlay-bottom
type: task
status: draft
version: 0.0.1
summary: >
  Create a `codon-which-key` crate that registers a `ModalView`
  responding to `window.observe_pending_input`, rendering the
  possible bindings full-width across the active pane at the
  bottom edge, with multi-column layout. The companion task
  `phase-16/which-key-auto-flip` adds the top-flip rule.
owners: [carlo]
progress: done
refines:
  - REQ:codon/which-key-overlay#c-full-pane-width
  - REQ:codon/which-key-overlay#c-bottom-default
  - REQ:codon/which-key-overlay#c-multi-column
  - REQ:codon/which-key-overlay#c-group-rendering
  - REQ:codon/which-key-overlay#c-suppress-zed
  - REQ:codon/which-key-overlay#c-settings-flag
  - REQ:codon/which-key-overlay#c-respect-delay
  - REQ:codon/which-key-overlay#c-dismiss-on-input-clear
  - REQ:codon/which-key-overlay#c-mode-aware-title
aspects: [width, anchor-bottom, multi-column, grouping, suppress-zed, settings, delay, dismiss, mode-title]
---

# Codon which-key overlay (bottom variant)

## What changes

Create a new workspace crate `crates/codon-which-key/`:

```
crates/codon-which-key/
  Cargo.toml
  src/
    codon_which_key.rs         lib root (init, settings, FILTERED_KEYSTROKES)
    codon_which_key_modal.rs   ModalView impl + render
    codon_which_key_settings.rs settings shape (enabled, delay_ms, flip_threshold, min_column_width)
```

The bulk of the work is a faithful port of
[`vendor/zed/crates/which_key/src/which_key_modal.rs`](spec:src:vendor/zed/crates/which_key/which_key_modal.rs)
with three rendering changes:

1. **Width = active pane width**, not `viewport_width * 0.5`. Read
   the active pane via
   `workspace.active_pane().read(cx).pixel_position_of_cursor`-style
   bounds — or via the new `Workspace::active_pane_bounds()`
   accessor if codon's `workspace::codon_bridge` doesn't already
   expose one (add it if missing — small additive surface
   following the convention in
   [`workspace::codon_bridge`](spec:src:vendor/zed/crates/workspace/src/codon_bridge.rs)).
2. **Multi-column layout.** Replace the current single-column
   `h_flex` (keystroke column + action column) with a grid sized
   to `floor(pane.bounds.size.width / min_column_width)` columns
   (`min_column_width` configurable, default ~240 px). Each column
   holds N rows; rows flow column-first so the visual reading
   order is "top-to-bottom, then left-to-right" — mirrors Helix.
3. **Mode-aware title.** Prefix the pending-keys label with the
   current `codon_mode::CodonModeTracker` pane-mode
   (`[NORMAL]`, `[INSERT]`, `[COMMAND]`, `[SELECT]`) so the user
   sees which mode the chord is firing under.

The pending-input plumbing (subscription, dismiss-on-clear,
delay-timer) is copied verbatim from
[`vendor/zed/crates/which_key/src/which_key.rs`](spec:src:vendor/zed/crates/which_key/src/which_key.rs)
and slightly re-keyed onto codon's settings struct.

Wire in `apps/codon/src/main.rs`:

```diff
-    which_key::init(cx);
+    codon_which_key::init(cx);
```

(or simply *don't* call `which_key::init` — verify by reading
`apps/codon/src/main.rs` for the current call site.)

Settings carry-over in `assets/config/codon.example.toml`:

```toml
[which_key]
enabled = true
delay_ms = 250            # match Zed's default
min_column_width = 240
flip_threshold = 0.5      # threshold for phase-16/which-key-auto-flip
```

## Why this clause

The vendored which-key is functional but cosmetically wrong for
codon's tiled-pane layout — its 480 px max width disappears into
the lower-right corner of a 4-pane layout and almost never aligns
with the pane the user is typing into. A full-width strip across
the active pane is the only positioning that scales with the
multiplexer.

The multi-column rendering is what lets the HUD actually replace
the vendored cheatsheet for chord families like `g …` (~20
bindings) and `space …` (~15 bindings) without scroll.

## Verification

- Boot codon, press `cmd-k`. The HUD should appear after
  `delay_ms`, spanning the full width of the active pane, with
  columns of `prefix h`, `prefix s n`, `prefix shift-w …` etc.
- Set `[which_key] enabled = false` in `codon.toml` and reload.
  The HUD never appears; chord prefixes still work.
- Press `cmd-k`, then any non-prefix key. The HUD dismisses.
- Multiple panes, focus the smallest one, press `cmd-k`. The HUD
  still respects the active pane's bounds (auto-flip is the
  follow-on task).
- `cargo clippy -p codon-which-key` reports no warnings.

## Done when

- `codon-which-key` builds, links into `apps/codon`, and replaces
  the vendored `which_key::init` call.
- The HUD spans the active pane width and uses multi-column
  rendering.
- The mode label prefixes the pending-keys line.
- `[which_key]` settings round-trip via `codon.toml`.
- A snapshot test (or a small unit test exercising
  `compute_columns(pane_width, min_column_width, binding_count)`)
  guards the multi-column logic.
- `spec lint` is at zero errors.
