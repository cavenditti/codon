---
id: TASK:phase-5/codon-buffer-second-consumer
type: task
status: superseded
version: 0.2.0
summary: >
  Wontdo 2026-05-13 — Helix-as-engine integration was removed from
  the roadmap (codon uses Zed's built-in Helix-style modal editing
  instead). No second consumer is planned, so the trait has no path
  to earning its abstraction. REQ:codon/buffer-trait is now
  superseded; the codon-buffer crate itself is slated for removal
  under a follow-up cleanup.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/buffer-trait#c-helix-impl
  - REQ:codon/buffer-trait#c-consumer-rewire
  - REQ:codon/code-quality#c-speculative-abstractions
aspects: [helix-impl-stub, consumer-migration, abstraction-watchlist]
---

# `codon-buffer`: earn the abstraction or defer it

## What ships

[`crates/codon-buffer/`](spec:src:crates/codon-buffer) defines a
`Buffer` trait with 12 read-only methods (snapshot, text_snapshot,
file, language, capability, is_dirty, saved_version, saved_mtime,
encoding, has_bom, anchor_before, anchor_after) and exactly one
implementer: a 46-line forwarder for `language::Buffer`. No codon
crate currently consumes `dyn Buffer`. Greps on `dyn codon_buffer::Buffer`
and `&dyn Buffer` (qualified to this crate) return zero hits.

The intent (per the module doc comment and
REQ:codon/buffer-trait) is to abstract over text buffers so Helix's
`Document` can plug in as a second implementer. That work has not
started. Until it does, the trait is speculative infrastructure
that has to track every change to `language::Buffer`'s public
surface for no consumer benefit.

This is the case that REQ:codon/code-quality#c-speculative-abstractions
exists to catch. This task is the explicit tracking record.

## Resolution paths

Pick one before the end of phase-5:

1. **Earn it** — land at least the skeleton of the Helix-`Document`
   impl in `codon-buffer`, even if Helix integration isn't yet
   complete. That gives the trait its second implementer and locks
   in the surface. Mark this task `done`.
2. **Earn it via consumer** — convert at least one codon-* call
   site that currently takes `&language::Buffer` to take
   `&dyn codon_buffer::Buffer`. That alone proves the trait is
   load-bearing for codon (not just for the future Helix port).
   Mark this task `done`.
3. **Defer** — if neither (1) nor (2) is on the phase-5 critical
   path, mark this task `deferred` with a target re-evaluation
   date. The trait stays in the workspace but is documented as
   speculative; `REQ:codon/buffer-trait` shifts from `accepted` to
   `draft` to reflect that nothing depends on it yet.
4. **Wontdo** — if Helix integration drops off the roadmap entirely
   (no longer a phase-6 / phase-7 plan), remove the crate and mark
   this task `wontdo`. REQ:codon/buffer-trait moves to
   `superseded`.

## File anchors

- [`crates/codon-buffer/src/codon_buffer.rs`](spec:src:crates/codon-buffer/src/codon_buffer.rs)
  — the trait and the lone `language::Buffer` impl.
- [`vendor/helix/`](spec:src:vendor/helix) — the second-impl target.

## Acceptance

This task does not have a single "ship" — the resolution path is the
acceptance criterion. The task closes (`done`, `deferred`, or
`wontdo`) before phase-5 closes.

Effort: depends on path. (1) is large (multi-week, blocked by Helix
integration work). (2) is small (one consumer migration). (3) and
(4) are bookkeeping.
