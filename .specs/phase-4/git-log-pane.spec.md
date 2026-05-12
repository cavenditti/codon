---
id: TASK:phase-4/git-log-pane
type: task
status: accepted
version: 0.0.2
summary: >
  Standalone git log pane — adopted as-is from Zed's existing
  GitGraph workspace::Item.
owners: [carlo]
progress: done
refines:
  - REQ:codon/git-pane#c-log
---

# Git log as a codon pane

## Outcome: adopt-as-is

Zed already ships a full git-log pane as
`git_graph::GitGraph`
([`vendor/zed/crates/git_graph/src/git_graph.rs:3118`](spec:src:vendor/zed/crates/git_graph/src/git_graph.rs))
— a `workspace::Item` with ASCII graph rendering on the left, `<sha>
<subject> (<author>)` on the right, keyboard nav, and Enter to open
the commit's diff. It registers itself on the workspace at
`git_graph.rs:747`.

No codon-side work needed. The pane is reachable today via the
`git_graph::Open` action; users who want a fixed chord can rebind it
in `~/.config/codon/codon.toml` under `[bindings.global]`. The
original TASK's framing — "extract log-rendering bits into a
standalone pane under codon-git" — was wasted scope; Zed already
solved this and the dock + log graph together cover the
navigation surface.

If we later want to apply the GitPanel-style modal patch (publish
`pane_mode`, sync `CodonModeTracker`, add `[bindings.git_graph.*]`),
that'd be a small follow-up — not blocking.
