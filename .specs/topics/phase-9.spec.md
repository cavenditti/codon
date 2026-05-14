---
id: TOPIC:topics/phase-9
type: topic
status: draft
version: 0.0.1
summary: >
  Editor jump-to-word discoverability + file-manager visual polish.
owners: [carlo]
---

# Phase 9 — Editor jumps + file-manager polish

Two narrow tracks:

- Surface the upstream Helix `gw` jump-to-word in codon's curated
  keymap so it appears in the cheatsheet, and verify the label color
  reads well against codon's palette.
- Add yazi-style visual polish to the file manager: per-filetype
  filename colors, stronger git-status tint, mode badge, marked-row
  stripe, cursor-row contrast, header sort/filter chips.

Refining requirements:

- [REQ:codon/editor-jumps](spec:REQ:codon/editor-jumps) — surface
  `g w` / `g W` in codon's curated cheatsheet.
- [REQ:codon/file-manager-theme](spec:REQ:codon/file-manager-theme) —
  filetype colors, git-status emphasis, mode badge, marked stripe,
  cursor contrast, header chips.
