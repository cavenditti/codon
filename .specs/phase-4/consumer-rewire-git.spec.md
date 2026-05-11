---
id: TASK:phase-4/consumer-rewire-git
type: task
status: accepted
version: 0.0.1
summary: >
  Rewire git_ui's Buffer callsites to take &dyn codon_buffer::Buffer
  at trait boundaries. Prerequisite for the git pane work below.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/buffer-trait#c-consumer-rewire
---

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
