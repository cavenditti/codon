---
id: TOPIC:topics/phase-14
type: topic
status: draft
version: 0.0.1
summary: >
  Spec-graph hygiene — reconcile historical commit Spec-Ref trailers
  that point at ids renamed or removed during phase-5 cleanup, so
  `spec lint` returns zero errors on the master tree.
owners: [carlo]
---

# Phase 14 — Spec-graph hygiene

Codon's `.specs/` graph picked up nine `R013` lint errors when the
phase-5 cleanup renamed a handful of REQ/TASK/TOPIC ids without
leaving forwards in place. The trailers in those nine historical
commits still point at the old names, and `spec lint` flags every
one as a dangling reference. Phase 14 closes the gap two ways:

- **Redirects** for ids that simply moved (typo'd phase trailers
  pointing at `TOPIC:phase-6` instead of `TOPIC:topics/phase-6`,
  clause names that gained the `c-` prefix mid-phase).
- **Placeholder `wontdo` / `superseded` specs** for ids that were
  never adopted (`REQ:codon/window-chrome`,
  `REQ:codon/keyboard-only-ui`, `REQ:codon/branding`,
  `TASK:phase-N/terminal-scrollbar`). The placeholder preserves the
  historical trail without re-litigating the original decision.

Once both passes land, the `#c-spec-lint-clean` clause on
[REQ:codon/code-quality](spec:REQ:codon/code-quality) is satisfied
and the pre-commit hook stops emitting errors on otherwise-clean
trees.

Refining requirements:

- [REQ:codon/code-quality](spec:REQ:codon/code-quality) — clause
  `#c-spec-lint-clean`.
