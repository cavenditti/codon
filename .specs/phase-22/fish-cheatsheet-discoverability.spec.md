---
id: TASK:phase-22/fish-cheatsheet-discoverability
type: task
status: accepted
version: 0.1.0
summary: >
  Add a "Shell" section to the codon cheatsheet (`cmd-k F1`)
  listing `codon do`, the convenience helpers, `#@` syntax + the
  trigger key. Plumb `codon do --help` and a `codon fish-init
  --print` for offline doc.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-discoverability
aspects: [cheatsheet-section, cli-help]
blocked_by:
  - TASK:phase-22/fish-action-dispatch
  - TASK:phase-22/fish-hash-at-trigger
---

# Fish surfaces in cheatsheet + CLI help

## Plan

- Cheatsheet section:
  - Extend
    [crates/codon-keymap/src/cheatsheet_modal.rs](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)
    with a new "Shell" tab/section. Surfaces:
    - `codon do <action>` — generic action dispatcher.
    - `codon edit <path>` — open file in editor pane.
    - `codon split [direction]` — split current terminal.
    - `codon win <n|next|prev>` — window navigation.
    - `codon fm [path]` — open file manager.
    - `#@ <desc>` + Ctrl-G — agent-complete a command.
    - `<partial> #@ <desc>` + Ctrl-G — agent-complete with a
      prefix.
  - Each row's "binding" column is the literal shell syntax,
    not a key chord. The cheatsheet's existing column layout
    accommodates this (the binding column is already a
    free-form string).
- CLI help:
  - `codon do --help` lists the action dispatch usage + an
    example.
  - `codon fish-init --help` documents the installer flags.
  - `codon fish-init --print` writes the plugin to stdout so
    the user can pipe / inspect it.
- A new top-level `docs/shell-integration.md` page with
  expanded examples. Linked from the cheatsheet's Shell
  section header (the section's footer shows
  `see: docs/shell-integration.md` as plain text — no
  hyperlink in a terminal modal).

## Acceptance

- `cmd-k F1` shows the Shell section with the documented
  entries.
- `codon do --help` exits 0 with usage text.
- `codon fish-init --print` writes a valid fish script to
  stdout.
- `docs/shell-integration.md` exists.
- `cargo test -p codon-keymap` passes.
