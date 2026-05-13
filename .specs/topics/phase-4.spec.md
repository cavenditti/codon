---
id: TOPIC:topics/phase-4
type: topic
status: draft
version: 0.2.0
summary: >
  Git integration (panel + diff pane) and the unified TOML config.
  The buffer-trait sub-goal is superseded — Helix-as-engine
  integration was removed from the roadmap (codon uses Zed's
  built-in Helix-style modal editing instead).
owners: [carlo]
---

# Phase 4 — Git integration & unified config

The original phase-4 framing also planned a codon Buffer trait to
decouple consumers from `language::Buffer`, enabling a future
Helix-as-engine integration. That sub-goal is **superseded as of
2026-05-13** — codon adopted Zed's built-in Helix-style modal
editing (vim mode with `helix_default` force-enabled) wholesale
rather than vendoring helix-editor. See
`REQ:codon/buffer-trait` for the historical record.

Refining requirements (still in flight):

- [REQ:codon/git-pane](spec:REQ:codon/git-pane)
- [REQ:codon/unified-config](spec:REQ:codon/unified-config)

Superseded sub-goal (kept for traceability):

- [REQ:codon/buffer-trait](spec:REQ:codon/buffer-trait) — superseded
