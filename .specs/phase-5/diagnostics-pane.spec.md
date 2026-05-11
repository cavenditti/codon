---
id: TASK:phase-5/diagnostics-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Register Zed's ProjectDiagnosticsEditor as a codon pane bound to a
  cmd-k chord, with codon-mode j/k navigation already supplied by the
  underlying Editor.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/additional-panes#c-diagnostics
---

# Diagnostics pane

## What ships

A pane listing all LSP diagnostics across the workspace, openable
from any context.

## Where it comes from

- [`vendor/zed/crates/diagnostics/src/diagnostics.rs`](spec:src:vendor/zed/crates/diagnostics/src/diagnostics.rs)
  already defines `ProjectDiagnosticsEditor` (a Render view backed by
  a multibuffer). j/k navigation works because it's an Editor under
  the hood and Helix mode is force-on in codon.

## Approach

1. Confirm `diagnostics::init(cx)` runs in `apps/codon/src/main.rs`.
2. Register an action `diagnostics::OpenDiagnostics` (already exists
   upstream as `diagnostics::Deploy`); bind it in codon-keymap.
3. Default keymap: `cmd-k d g` (`d` for diagnostics, `g` to avoid
   clashing with future `cmd-k d d` for diff viewer).

Net: < 20 LOC, mostly a keymap line plus a `bind!` arm in the
codon-keymap resolver.
