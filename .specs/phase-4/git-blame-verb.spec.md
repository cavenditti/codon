---
id: TASK:phase-4/git-blame-verb
type: task
status: accepted
version: 0.0.2
summary: >
  Git blame — adopted as-is from Zed's existing inline-blame
  toggle on the editor. The Selection::Hunks cross-pane verb
  shape from the original framing is deferred.
owners: [carlo]
progress: done
refines:
  - REQ:codon/git-pane#c-blame-verb
---

# git.blame.show cross-pane verb

## Outcome: adopt-as-is (with reduced scope)

Zed already gives us blame: `editor::ToggleGitBlame` and the
`git::Blame` action toggle inline blame gutters on any editor with a
git-tracked buffer. The renderer lives in
[`vendor/zed/crates/editor/src/git/blame.rs`](spec:src:vendor/zed/crates/editor/src/git/blame.rs)
and works today inside codon without any patch.

The original TASK framing — a new `codon_git::BlameShow` action that
*consumes `Selection::Hunks`* and renders blame metadata per hunk —
was contingent on the Phase-3 selection-first work
([REQ:codon/selection-first](spec:REQ:codon/selection-first))
that hasn't fully materialised. Building the cross-pane variant
before the Selection plumbing is stable would freeze design choices
we can't yet justify.

Net call: ship is "blame works in the editor, no codon-side work
required." When/if `Selection::Hunks` becomes a real surface, the
blame-per-hunk verb is a small follow-up — pull metadata via the
same path the inline renderer uses, render in a popover. Open as a
new TASK at that point rather than dragging this one open.
