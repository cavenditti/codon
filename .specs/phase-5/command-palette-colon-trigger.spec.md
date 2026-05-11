---
id: TASK:phase-5/command-palette-colon-trigger
type: task
status: accepted
version: 0.0.1
summary: >
  Bind `:` in codon Normal mode (terminal, file manager, editor) to
  open the codon command palette. cmd-shift-p continues to open the
  same modal as an alternative.
owners: [carlo]
progress: done
refines:
  - REQ:codon/command-palette#c-colon-trigger
---

# `:` trigger for the command palette

## What ships

- New action `codon_command_palette::Toggle` registered in the new
  `crates/codon-command-palette` crate.
- Bindings in `crates/codon-keymap/src/keymap.rs` (and the example
  config at `assets/config/codon.example.toml`):
  - `[bindings.terminal.normal]` `":"` → `codon_command_palette::Toggle`
  - `[bindings.file_manager.normal]` `":"` → `codon_command_palette::Toggle`
  - `[bindings.editor.normal]` (and `[bindings.global]`)
    `"cmd-shift-p"` → `codon_command_palette::Toggle`
- The action opens the codon modal (see `command-palette-modal`) and
  populates it from the global Zed `Action` registry, exactly as
  `command_palette::Toggle` does today.

## Reference points

- [`vendor/zed/crates/command_palette/src/command_palette.rs`](spec:src:vendor/zed/crates/command_palette/src/command_palette.rs)
  — `Toggle` action, `CommandPaletteDelegate` instantiation pattern.
- [`crates/codon-mode/src/mode.rs`](spec:src:crates/codon-mode/src/mode.rs)
  — Normal-mode context predicates already used by other codon
  bindings.

## Tests

- Manual: launch codon, focus a terminal, hit `:` → palette opens.
- Manual: same for file manager and editor.
- Manual: `cmd-shift-p` opens the same modal.

Effort: low. ~50 LOC including the action declaration.
