---
id: REQ:codon/window-chrome
type: requirement
status: superseded
version: 0.0.1
level: SHOULD
summary: >
  Superseded placeholder — the `[window]` titlebar drag / zoom
  controls landed as part of `codon-config` and the vendored Zed
  `platform_title_bar::WindowChromeConfig` global. No standalone REQ
  was ever authored; this file exists only to keep the historical
  commit 05cc9ab Spec-Ref trailer resolvable.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-14]
---

# Window chrome (superseded)

## Why this file exists

Commit `05cc9ab` ("feat(codon-config): [window] sub-tree controls
titlebar drag + zoom") used a `Spec-Ref:` trailer pointing at
`REQ:codon/window-chrome` before the REQ itself was drafted. The
work landed in code (a `WindowChromeConfig` global wired through
`codon-config`'s `[window]` table) and the standalone REQ was never
written — the surface is small enough that the config schema in
`assets/config/config.example.toml` is the canonical reference.

## What's actually configurable

Two boolean knobs under `[window]` in `~/.config/codon/config.toml`:

- `disable_drag` — mouse-drag the titlebar becomes a no-op.
- `disable_double_click_zoom` — double-click titlebar becomes a
  no-op.

Both default to `false`. Combined with hidden traffic lights, they
make the titlebar strip fully inert (keyboard window placement only)
without losing OS-level window-manager hooks for keyboard users.

## Resolution

This placeholder satisfies `R013` for the legacy trailer; no
follow-up work is planned. Future window-chrome changes go through
the existing `codon-config` schema, not through new clauses here.
