---
id: TASK:phase-16/which-key-auto-flip
type: task
status: draft
version: 0.0.1
summary: >
  Extend the `codon-which-key` overlay with an auto-flip rule:
  when the natural content height would exceed
  `pane.bounds.size.height * flip_threshold`, render at the top
  edge of the active pane instead of the bottom. Threshold is
  configurable; default 0.5.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/which-key-overlay#c-auto-flip
---

# Which-key overlay auto-flip rule

## What changes

In `crates/codon-which-key/src/codon_which_key_modal.rs::render`,
after computing the content height (sum of `row_count * row_height`
plus the title section, all clamped to the configured `max_h`),
compare against the active pane's height:

```rust
let active_pane_height = active_pane_bounds.size.height;
let threshold = settings.flip_threshold;          // default 0.5
let would_occlude = content_height > active_pane_height * threshold;
let anchor = if would_occlude { Anchor::Top } else { Anchor::Bottom };
```

Render positioning becomes anchor-aware:

```rust
let positioned = match anchor {
    Anchor::Bottom => div().absolute()
        .bottom(active_pane_bottom_offset)
        .left(active_pane_bounds.origin.x)
        .w(active_pane_bounds.size.width),
    Anchor::Top => div().absolute()
        .top(active_pane_bounds.origin.y)
        .left(active_pane_bounds.origin.x)
        .w(active_pane_bounds.size.width),
};
```

The `bottom_offset` math stays the same as the bottom-variant task
— status-bar clearance + a small margin. The top anchor uses no
extra clearance; the HUD sits flush against the top of the pane.

Edge cases the rule must handle:

- **Pane height is small** (< ~10 rows of HUD content). Threshold
  triggers; flip to top. The HUD may still occlude — that's fine,
  the user can dismiss with escape.
- **Pane covers full window** (single-pane layout). The threshold
  still applies against the pane height, so a chord family that
  needs a tall HUD still flips. This is correct: it preserves the
  invariant that the HUD never eats more than `threshold` of the
  visible content area.
- **Pane shrinks while HUD is up** (rare — resize during chord
  hold). The modal re-renders on `observe_pending_input` ticks but
  not on resize. Acceptable: re-evaluate anchor each render call,
  but don't subscribe to pane resize separately.

Settings:

```toml
[which_key]
flip_threshold = 0.5    # 0.0..1.0 — fraction of pane height
```

Add validation in `codon_which_key_settings`: clamp to `0.1..0.9`
on load with a warn-log if out of range. A threshold of 0 makes the
HUD always flip; 1 means it never flips — both are foot-guns
without a strong use case.

## Why this clause

The bottom-default position is fine for tall panes but actively
hostile in short ones (split-bottom terminal pane is often 8-12
lines tall — a 6-row HUD eats half of it). Auto-flipping to the
top keeps the HUD visible without burying the pane content
underneath it, and the configurable threshold lets users tune the
sensitivity to taste.

## Verification

- Open codon with a tall single pane. Press `cmd-k`. HUD anchors
  bottom.
- Resize the pane to <30 % of the window height. Press `cmd-k`.
  HUD anchors top (or below — depends on threshold; at default
  0.5 a sufficiently dense chord family will flip).
- Set `[which_key] flip_threshold = 0.2` in `codon.toml`. Reload.
  HUD now flips more aggressively (top anchor even in
  larger panes).
- Set `flip_threshold = 0.95`. HUD almost never flips.
- A unit test on
  `should_flip(content_h: Pixels, pane_h: Pixels, threshold: f32) -> bool`
  covers boundary values (0.0, 1.0, exactly at threshold).

## Done when

- The HUD anchors top when the rule fires; bottom otherwise.
- Threshold is configurable via `codon.toml` and clamped on load.
- The boundary-value test passes.
- `spec lint` is at zero errors.
