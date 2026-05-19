---
id: TASK:phase-19/stacked-panes-member-stack
type: task
status: draft
version: 0.0.1
summary: >
  Add `Member::Stack { panes, active }` variant to vendored
  `workspace::pane_group`, walk every match-site to handle it
  (no `_ =>` wildcards), and make `LayoutSnapshot::Stack`
  round-trip without degrading to active-member-only on apply.
  Rendering, cycle actions, and overview shorthand are
  follow-up tasks.
owners: [carlo]
progress: done
refines:
  - REQ:codon/stacked-panes#c-member-stack
  - REQ:codon/stacked-panes#c-match-site-coverage
  - REQ:codon/stacked-panes#c-layout-snapshot
aspects: [member-stack-variant, match-site-walk, snapshot-round-trip]
---

# Member::Stack variant + match-site walk + snapshot round-trip

## What ships

The structural refactor in vendored Zed that lifts the
"deferred" footnote in `REQ:codon/layout#c-stack-live-rendering`
into a real, end-to-end-coherent data model. Live rendering and
the keyboard verbs ride on top in separate tasks
(`phase-19/stacked-panes-render`,
`phase-19/stacked-panes-actions`,
`phase-19/stacked-panes-overview-shorthand`).

1. **`Member::Stack { panes: Vec<PaneId>, active: usize }`** in
   `vendor/zed/crates/workspace/src/pane_group.rs`. Sibling to
   the existing `Member::Pane` and `Member::Axis`. Invariant:
   `panes.len() >= 2` — a single-pane "stack" is just
   `Member::Pane`; helper functions enforce this on construction.

2. **`Member::active_pane()` helper** that returns the visible
   pane for all variants (`Pane(p) => p`, `Stack { panes, active }
   => panes[active]`, `Axis { ... }` panics — match-sites that
   care about Axis vs leaf handle it explicitly).

3. **Match-site walk** — every match on `Member` in vendored
   Zed handles the Stack arm. The estimated set is ~15 sites
   across `pane_group.rs`, `workspace.rs`, `pane.rs`, and
   `codon_bridge.rs`. The walk is the bulk of this task; sites
   that genuinely don't care about Stack-vs-Pane delegate to
   `active_pane()`. Wildcards (`_ =>`) are not allowed to swallow
   the new variant.

4. **`LayoutSnapshot::Stack` round-trip** in
   `workspace::codon_bridge`. `capture_layout` produces
   `Stack { members, active }` for a `Member::Stack`; `apply_layout`
   reconstructs a `Member::Stack` rather than dropping the inactive
   members.

5. **Forward-compatibility** — old saved sessions that captured a
   stack as "active member only" (the current degraded behaviour)
   still deserialise: the apply path tolerates `Stack { members:
   vec![one], active: 0 }` by emitting a `Member::Pane` instead.

## Out of scope

- Live rendering of the stack tab strip.
- Keybindings for cycle / add / remove / promote.
- Overview-modal shorthand updates.
- Per-stack `active` index persistence beyond what the existing
  KVP write already covers.

## Coordination

This TASK lives in `vendor/zed/`. Per CLAUDE.md and the existing
`phase-14/codon-bridge-single-registry` workflow:

1. Commit on the `codon` branch inside `vendor/zed/`
   (Conventional commit `feat(pane_group): add Member::Stack` or
   `refactor(pane_group): …`).
2. Run `( cd vendor/zed && ./script/clippy )` — clean.
3. Commit the submodule-pointer bump in the outer repo.

## Verification

- `cargo build -p workspace` (vendored) clean.
- `( cd vendor/zed && ./script/clippy )` clean.
- `cargo build -p codon` clean — codon-side callers in
  `codon-panes`, `codon-session`, and elsewhere compile against
  the new Member shape.
- Unit test in `pane_group.rs`: construct a `Member::Stack`, walk
  it through `active_pane()`, confirm correct pane returned.
- Snapshot test: build a layout containing a Stack of three panes
  with `active = 1`, capture → apply → re-capture, assert the
  round-tripped snapshot equals the original.
- Smoke: hand-construct a session JSON with a `Stack` entry via
  the KVP store, restart codon, verify all three stack members
  exist in memory (visible via a temporary `:debug layout`
  command).

## Files touched

- `vendor/zed/crates/workspace/src/pane_group.rs` — new variant
  + helper.
- `vendor/zed/crates/workspace/src/workspace.rs` — match-site
  coverage.
- `vendor/zed/crates/workspace/src/pane.rs` — match-site
  coverage.
- `vendor/zed/crates/workspace/src/codon_bridge.rs` — snapshot
  capture / apply paths.
- Any other vendored match-site that `rg -n 'match .*Member' vendor/zed/crates/workspace`
  surfaces.
- Outer repo: submodule bump.
