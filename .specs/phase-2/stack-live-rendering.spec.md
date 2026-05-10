---
id: TASK:phase-2/stack-live-rendering
type: task
status: accepted
version: 0.1.0
summary: >
  First-class Member::Stack variant in vendored pane_group, with no-close-X tab strip and stack actions.
owners: [carlo]
progress: deferred
refines:
  - REQ:codon/layout#c-stack-live-rendering
---

# Stack live rendering (deferred)

Needs Member::Stack added to the Member enum in vendor/zed/crates/workspace/src/pane_group.rs, plus updates to ~15 match sites across pane.rs, workspace.rs, and the persistence model. Held until a quieter window — the snapshot-level fallback already lets us serialize and round-trip a Stack.
