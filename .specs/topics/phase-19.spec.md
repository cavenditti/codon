---
id: TOPIC:topics/phase-19
type: topic
status: draft
version: 0.0.1
summary: >
  Selection-first depth (terminal blocks as typed objects, typed
  selection registers, the pane-level ObjectGrammar trait) plus
  first-class stacked panes — the design surface from
  `codon-architecture.typ` §6 and §7 that the existing REQs
  scaffold but don't yet cover.
owners: [carlo]
---

# Phase 19 — Selection-first depth + first-class stacks

Phases 1–18 shipped the selection-first foundations
(`REQ:codon/selection-first`: ObjectKind, Selection,
SelectionSource, ActionAcceptsRegistry) and the LayoutSnapshot
round-trip with a deferred Stack variant
(`REQ:codon/layout#c-stack-fallback`). The codon design doc's
Section 6 ("Selection-first interaction") and Section 7
("Sessions, windows, and layout") describe further machinery —
typed registers, the `ObjectGrammar` refinement trait, terminal
blocks as a typed object kind, and live `Member::Stack`
rendering — that the spec graph doesn't yet hold.

Phase 19 closes those four gaps in one themed block. None of
them are urgent (today's tabs cover stacks; today's three
agent verbs cover the cross-pane payoff; text registers via the
Helix code path cover the common case). Together they unlock
the Section 6 promise: *the same alphabet, the same muscle
memory, across every pane*.

Refining requirements:

- [REQ:codon/terminal-blocks](spec:REQ:codon/terminal-blocks) —
  OSC 133 + heuristic block boundary detection, the `Block`
  typed object, terminal-pane `SelectionSource` for blocks,
  block-aware navigation, and the cross-pane payoff
  (`codon_agent::Explain` on the failing block).
- [REQ:codon/selection-registers](spec:REQ:codon/selection-registers) —
  typed-selection registers, per-session by default and named-
  persistent across sessions. Interops with Helix's text
  registers from the vendored vim crate.
- [REQ:codon/object-grammar](spec:REQ:codon/object-grammar) —
  the `ObjectGrammar` trait per pane kind so `w` / `b` / `mi…`
  / `s` / `K` work as refinement operators over each pane's
  native objects, not just over text.
- [REQ:codon/stacked-panes](spec:REQ:codon/stacked-panes) —
  add `Member::Stack` to the vendored `pane_group`, render it
  live with a no-close-X tab strip, and stop degrading
  `LayoutSnapshot::Stack` to active-member-only on apply.
