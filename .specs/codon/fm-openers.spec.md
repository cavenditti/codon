---
id: REQ:codon/fm-openers
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Per-extension / per-mime opener configuration — choose-opener picker
  (`O`), `openers.toml` declaration, default Enter-route consults it.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-7]
---

# File manager openers

Today the file manager always sends Enter through
`workspace.open_abs_path`, which delegates to Zed's project-item
registry. That's the right default but offers no escape hatch when
the user wants a specific opener (`xdg-open`, `qlmanage -p` on macOS,
`code -g foo:42`, the user's image viewer, …).

:::{requirement id="fm-openers" level="SHOULD"}
The file manager SHOULD support:

- {#c-choose-opener} `O` (shift-o) on a file shows a picker of
  every opener whose glob / mime matches the selected entry, plus
  a synthetic "Codon (default)" entry that runs the current
  `open_abs_path` path. Marked-set semantics: when marks exist,
  the chosen opener runs for each.
- {#c-opener-config} `~/.config/codon/openers.toml`:
  ```toml
  [[opener]]
  glob = "*.{png,jpg,jpeg}"
  cmd  = "qlmanage -p {path}"
  block = false   # spawn detached vs await exit
  description = "Quick Look"
  ```
  Loaded once at startup and on FS-watcher notifications. Writeback
  via the `toml_edit` AST flow already used by codon-config so
  user edits survive in-app changes. Substitutions: `{path}`,
  `{paths}` (joined), `{cwd}`, `{parent}`.
- {#c-default-opener-route} on Enter / `l`, the FM consults the
  opener config first. If a non-`Codon (default)` opener matches
  and is unique, it runs without prompting. If multiple match, the
  user gets the `O` picker. If none match, the current
  `open_abs_path` path runs unchanged.
:::
