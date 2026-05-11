---
id: TOPIC:topics/phase-5
type: topic
status: draft
version: 0.1.0
summary: >
  Native UX coverage — file manager polish, additional panes (diff,
  image, diagnostics), and full replacement of OS-native dialogs.
owners: [carlo]
---

# Phase 5 — Native UX coverage

The catch-all for ergonomic gaps. Filling out the file manager (fuzzy
filter, git status indicators, copy/paste files), adding pane types
that complete the workflow loop (diff viewer, image preview, diagnostics
panel), and finally replacing every OS-native file dialog with an
in-app picker.

Refining requirements:

- [REQ:codon/fm-enhancements](spec:REQ:codon/fm-enhancements)
- [REQ:codon/additional-panes](spec:REQ:codon/additional-panes)
- [REQ:codon/in-app-pickers](spec:REQ:codon/in-app-pickers)
- [REQ:codon/command-palette](spec:REQ:codon/command-palette) —
  `:`-triggered palette with description pane and typed-argument
  completers (Layer A; typed-action upstream work deferred).
