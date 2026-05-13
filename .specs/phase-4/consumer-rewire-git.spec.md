---
id: TASK:phase-4/consumer-rewire-git
type: task
status: accepted
version: 0.0.1
summary: >
  Rewire git_ui's Buffer callsites to take &dyn codon_buffer::Buffer
  at trait boundaries. Prerequisite for the git pane work below.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/buffer-trait#c-consumer-rewire
---

## Wontdo (2026-05-13)

Helix-as-engine integration was removed from the roadmap — codon
adopted Zed's built-in Helix-style modal editing (vim mode with
`helix_default` force-enabled) rather than vendoring helix-editor.
The buffer-trait abstraction exists only to support a second
buffer impl (`helix_view::Document`); with that goal gone, there
is no future caller to justify rewiring git_ui through
`&dyn codon_buffer::Buffer`. `REQ:codon/buffer-trait` is now
superseded; this task closes alongside it.

The historical findings below are retained for the spec graph.

## Original deferral note (2026-05-12)

A survey of `git_ui` after the buffer-trait skeleton landed turned up
no `&language::Buffer` direct parameters to rewire. Buffer usage is:

- `Entity<Buffer>` parameters in four async functions
  (`text_diff_view::{build_clipboard_buffer, update_diff_buffer}`,
  `file_diff_view::build_buffer_diff`,
  `commit_view::build_buffer_diff`) — `Entity<T>` needs a concrete `T`
  so `&dyn Buffer` substitution doesn't apply without a separate
  entity-erasure abstraction.
- Two `&language::BufferSnapshot` parameters
  (`commit_view::extend_buffer_header_context_menu`,
  `git_panel:6079` rendering helper) — these take *snapshots*, not
  buffers, and only call `.file()` on the snapshot. The trait
  surface is on `Buffer`, not `BufferSnapshot`.

The "mostly mechanical" framing in the original TASK overestimated
how often `&language::Buffer` appears as a direct trait boundary in
git_ui. Doing the rewire well requires either:

1. Adding a parallel `BufferReader`-style trait that abstracts over
   `Entity<Buffer>` (read-with semantics), OR
2. Restructuring the git diff plumbing so snapshots cross the
   boundary instead of entities.

Both are bigger changes than this TASK scopes, and the parent
`REQ:codon/buffer-trait` is `SHOULD`-level — the trait + Zed impl
are still useful foundations for new code paths (e.g. the upcoming
git panes can take `&dyn codon_buffer::Buffer` from day one). When
a Helix `Document` impl actually lands, we'll revisit this with a
concrete entity-erasure strategy.

# git_ui consumer rewire

## Scope

Only `git_ui` is in scope for Phase 4. `editor` and `agent_ui` keep
their concrete `&language::Buffer` signatures — they work today and
nothing blocks on them changing. Rewiring them is a future task when
Helix lands.

## Files

The grep hits in git_ui to rewire (from exploration):

- [`vendor/zed/crates/git_ui/src/git_panel.rs`](spec:src:vendor/zed/crates/git_ui/src/git_panel.rs)
  — uses `.snapshot()`, `.text()`, `.is_dirty()`, `.edit()`,
  `.set_language()`, `.anchor_before()` / `.anchor_after()`, `.len()`
- The various diff views — same shape, less surface

## Approach

Change function signatures at trait boundaries (public API of
internal helpers) to `&dyn codon_buffer::Buffer`. Leave inherent
methods on `language::Buffer` alone — they're still callable on
concrete instances. The rewire is mostly mechanical: the Zed impl
satisfies every method already, so the existing callsites compile
unchanged once the function signatures shift.

`./script/clippy` should be clean after the change. No behavioural
diff expected.
