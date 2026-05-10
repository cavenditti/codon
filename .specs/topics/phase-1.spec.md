---
id: TOPIC:topics/phase-1
type: topic
status: accepted
version: 1.0.0
summary: >
  Modal shell, Helix mode, TOML keymap, and the file-manager pane —
  the foundational UX layer for codon.
owners: [carlo]
---

# Phase 1 — Modal shell & action layer

This phase brought up the always-modal experience on top of Zed:
PaneMode (Normal / Insert / Command), the codon-keymap TOML loader, the
selection-first foundation (Selection enum + SelectionSource trait +
ActionAcceptsRegistry), and the yazi-style three-column file manager.

All work is **complete and shipped**. Subsequent phases build on these
primitives — the modal model, the action registry, and the keymap
loader are extended (not replaced) in phases 2 and 3.

Refining requirements:

- [REQ:codon/modal-shell](spec:REQ:codon/modal-shell)
- [REQ:codon/file-manager](spec:REQ:codon/file-manager)
- [REQ:codon/selection-first](spec:REQ:codon/selection-first)
