---
id: TASK:phase-5/terminal-scrollbar
type: task
status: accepted
version: 0.0.1
summary: >
  Flip the vendored Zed terminal's scrollbar default so terminal
  panes get a visible scroll surface out of the box. Wontdo as a
  codon-side spec — the change is a one-line config flip inside
  `vendor/zed/` and there is no codon-crate work to track.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/code-quality#c-spec-lint-clean
---

# Terminal scrollbar default (wontdo)

## Why this file exists

Commit `cea6ef9` ("chore: bump vendor/zed for terminal scrollbar
default") used a `Spec-Ref:` trailer pointing at
`TASK:phase-N/terminal-scrollbar` — the phase number was never
filled in. The work itself is a single submodule-pointer bump on
`vendor/zed/` (codon branch); there is no codon-crate code path,
no clause to refine, and no follow-up task.

The `_redirects.toml` entry maps the literal `phase-N` trailer
onto this canonical `phase-5` task so `spec lint` resolves it.

## What the bump does

Inside the vendored Zed terminal element, the `show_scrollbar`
preference defaults to `true` instead of `false`. Users who want
the previous behaviour can flip it back via `terminal.show_scrollbar
= false` in their Zed settings — codon does not override this from
its own config surface.

## Resolution

This placeholder satisfies `R013` for the legacy trailer; the work
is complete (the submodule bump shipped in `cea6ef9`). No further
action.
