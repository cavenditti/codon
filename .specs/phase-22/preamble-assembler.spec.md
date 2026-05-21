---
id: TASK:phase-22/preamble-assembler
type: task
status: accepted
version: 0.1.0
summary: >
  Add `codon_agent::Preamble::build(workspace, cx) -> String` plus the
  fixed section ordering, version marker, and extensibility plumbing.
  All flows call this entry point — no flow assembles its own prefix.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-context-preamble#c-assembler-api
  - REQ:codon/agent-context-preamble#c-fixed-ordering
  - REQ:codon/agent-context-preamble#c-version-marker
  - REQ:codon/agent-context-preamble#c-extensible
aspects: [build-entry, section-order, version-marker, registry-trait]
---

# Preamble assembler skeleton

## Plan

- New module `crates/codon-agent/src/preamble/mod.rs` exposing
  `pub struct Preamble; impl Preamble { pub fn build(workspace:
  &Entity<Workspace>, cx: &mut App) -> String { ... } }`.
- Section assembly is a fixed sequence:
  1. `# codon-preamble v1` (version marker — bumped only on a
     breaking section change).
  2. `workspace: <root>` (canonicalised) + `codon: <build version>`.
  3. `session: <name>` + `window: <slot> "<label>"` (from
     `codon-session`).
  4. `pane: <kind> mode=<mode> slot=<slot>`.
  5. Pane-kind-specific snapshot (delegated to `PaneSnapshot` trait
     — sibling task `preamble-pane-snapshots`).
  6. Selection summary line if present (sibling task
     `preamble-selection-and-memories`).
  7. Surfaced memories block if any (same sibling task).
- The assembler MUST NOT contain a per-pane-kind match. Snapshot
  bodies come through a registry keyed on `PaneKind`. New pane
  kinds register their snapshot in their own `init(cx)` — same
  pattern as `codon_pane_kind_spec` in `workspace::codon_bridge`.
- Every section is a `&'static str` header concatenated with the
  body via `writeln!`. Section bodies that are empty *skip the
  header too* — keeps the preamble lean when a section has nothing
  to say.

## Acceptance

- Unit test: `Preamble::build` on a workspace with one terminal +
  one editor returns a string starting with
  `# codon-preamble v1\nworkspace: ...`.
- Unit test: snapshot order is exactly the documented sequence —
  reorder one section in the assembler, test fails.
- The phase-3 cross-pane verbs are migrated to call
  `Preamble::build` instead of their hand-rolled prefixes (the
  actual migration lands in `harness-migrate-existing-verbs`; this
  task only provides the entry point).
- `cargo test -p codon-agent` passes.
