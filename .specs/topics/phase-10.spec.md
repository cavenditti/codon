---
id: TOPIC:topics/phase-10
type: topic
status: draft
version: 0.0.1
summary: >
  Vimium-style global hint mode — every visible word, URL, and
  clickable UI element across the entire codon window gets a
  two-letter hint label; pressing two keys jumps the cursor /
  clicks the element / copies the URL.
owners: [carlo]
---

# Phase 10 — Global hint-mode jumps

Inspired by Vimium / Tridactyl / Helix `gw`. Two entry actions:

- `codon_jump::JumpToTarget` (`cmd-k j`) — any visible candidate;
  default action varies by candidate kind (editor word → move
  cursor, terminal word → focus + select, fm row → set cursor,
  UI button → click).
- `codon_jump::JumpToUrl` (`cmd-k u`) — URL candidates only; the
  selected URL is copied to clipboard with a toast.

Refining requirements:

- [REQ:codon/jump-hints](spec:REQ:codon/jump-hints) — overlay
  layer, provider trait, clickable wrapper, label assignment,
  keystroke loop, per-pane providers, config.
