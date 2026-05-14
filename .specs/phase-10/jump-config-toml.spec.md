---
id: TASK:phase-10/jump-config-toml
type: task
status: accepted
version: 0.0.1
summary: >
  `~/.config/codon/jump.toml` overrides — alphabet, label
  position, max candidates per provider, dismiss-on-scroll, and
  the two chords. FS-watch hot reload via the same pattern
  used by openers and file-manager-theme.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/jump-hints#c-config-toml
aspects: [config-loader, hot-reload, example-asset]
---

# Jump config TOML

## What ships

`JumpConfig` struct + `JumpConfigStore` `Global` in `codon-jump`,
mirroring `FmThemeStore` (openers / fm-theme already use the
pattern):

```toml
# ~/.config/codon/jump.toml
[jump]
alphabet      = "asdfghjkl;"   # default "abcdefghijklmnopqrstuvwxyz"
label_position = "top-left"     # | "center"
max_candidates_per_provider = 200  # safety cap
dismiss_on_scroll = true

[jump.chord]
target = "cmd-k j"
url    = "cmd-k u"
```

Loaded once at startup; FS-watched for hot reload. The chords are
echoed into the codon-keymap default set so cheatsheet reflects
the user's override (the keymap loader already supports user
overrides via `codon.toml`; this is the dedicated jump.toml for
non-keymap settings).

Defaults are the same hard-coded values from `jump-overlay-core`;
this task replaces those constants with calls to
`JumpConfigStore::current(cx)`.

`assets/config/jump.example.toml` ships as user-facing
documentation.

## Verification

- Default behavior unchanged: opens with full a-z alphabet,
  top-left labels.
- Edit `~/.config/codon/jump.toml`: alphabet shrinks to
  `asdfghjkl;`; next `cmd-k j` shows fewer-but-faster-to-type
  labels.
- Set `dismiss_on_scroll = false`: scrolling no longer cancels
  the overlay (useful for very long terminal scrollback flows).

## Where it slots in

- Edit: `crates/codon-jump/src/codon_jump.rs` — `JumpConfig` +
  `JumpConfigStore` + replace constants. ~120 LOC additive.
- New: `assets/config/jump.example.toml`.
