---
id: TOPIC:topics/phase-6
type: topic
status: draft
version: 0.0.1
summary: >
  Yazi-feature-parity for the file manager, wave 1 — navigation extras,
  sort/display modes, visual-range selection, richer preview.
owners: [carlo]
---

# Phase 6 — File-manager parity, wave 1

Codon's file manager today covers the basic three-pane / hjkl /
y-d-p / `/`-filter surface; it's roughly 30% of what yazi exposes.
Phase 6 closes the most user-visible gaps in pure UX-without-IO terms:
directory history, bookmarks, `:cd`-style goto, sort modes, line
modes, gitignore toggle, preview-ratio adjustment, visual-range
selection, select-all / invert, and richer preview (image / archive /
binary-info).

None of these depend on external tools (`fd`, `rg`, `zoxide`, …);
they extend the existing in-process FM model. Wave 2 (phase-7) picks
up everything that talks to an external indexer or opener.

Refining requirements:

- [REQ:codon/fm-nav-extras](spec:REQ:codon/fm-nav-extras) —
  history, goto path, reveal, bookmarks.
- [REQ:codon/fm-sort-display](spec:REQ:codon/fm-sort-display) —
  sort modes, line modes, gitignore toggle, preview ratio.
- [REQ:codon/fm-selection](spec:REQ:codon/fm-selection) —
  visual-range, select-all / invert, clear-marks.
- [REQ:codon/fm-preview-richer](spec:REQ:codon/fm-preview-richer) —
  image preview, archive listing, informative binary fallback.

## Idiomatically covered elsewhere (wontdo)

Yazi capabilities whose codon counterpart already exists outside the
file manager — surfaced here so the gap audit doesn't reopen them:

- Yazi **tabs** (`t` / `1-9` / `[` / `]`) → codon sessions and
  windows (`cmd-k s *`, `cmd-k shift-w *`).
- Yazi **help layer** (`~` / F1) → codon cheatsheet (`cmd-k F1`).
- Yazi **quit / cd-on-exit** → N/A; codon is an integrated workspace
  rather than a terminal-launchable file manager.
