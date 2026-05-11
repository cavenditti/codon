---
id: TASK:phase-4/unified-config-example-config
type: task
status: accepted
version: 0.0.1
summary: >
  Ship assets/config/codon.example.toml as the new template,
  superseding keymap.example.toml. Shows settings and bindings in
  one annotated file.
owners: [carlo]
progress: done
refines:
  - REQ:codon/unified-config#c-example-config
---

# Example unified config

## What ships

`assets/config/codon.example.toml` — a single annotated template
covering the most commonly tweaked settings (theme, fonts, editor
behaviour, terminal) plus every default codon binding. Replaces
`assets/config/keymap.example.toml`, which keeps a one-line redirect
comment for one release cycle:

```toml
# This file has been superseded by codon.example.toml.
# Place your overrides in ~/.config/codon/codon.toml.
```

## Sections to include

- `[settings.theme]`, `[settings.experimental]` (whatever Zed's
  current "popular settings" surface is)
- `[settings.terminal]`, `[settings.editor]`
- `[settings.languages.rust]` as a per-language override example
- `[bindings.global]` — every default codon chord with a comment
  per group (pane / sessions / windows / agent / git / help)
- `[bindings.editor.normal]`, `[bindings.terminal.normal]`,
  `[bindings.file_manager.normal]` — empty stubs with a one-line
  comment explaining how to override per-pane bindings

## Tests

Manual: copy to `~/.config/codon/codon.toml`, launch codon, confirm
no parse errors and the settings/bindings apply as documented.

Low. The bulk is documentation comments. ~250 LOC of TOML + prose.
