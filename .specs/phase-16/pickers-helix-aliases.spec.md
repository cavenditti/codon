---
id: TASK:phase-16/pickers-helix-aliases
type: task
status: draft
version: 0.0.1
summary: >
  Bind the Helix space-mode picker family under
  `prefix p <letter>` global chords in codon's `DEFAULT_KEYMAP`,
  re-using the existing Zed picker actions
  (`file_finder::Toggle`, `tab_switcher::Toggle`,
  `outline::Toggle`, `project_symbols::Toggle`,
  `diagnostics::Deploy`, `projects::OpenRecent`).
owners: [carlo]
progress: pending
refines:
  - REQ:codon/helix-pickers#c-prefix-namespace
  - REQ:codon/helix-pickers#c-cheatsheet-coverage
  - REQ:codon/helix-pickers#c-respect-pane-context
aspects: [bindings, cheatsheet, pane-context]
---

# Helix picker aliases under `prefix p`

## What changes

Add to `[bindings.global]` in
[`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
`DEFAULT_KEYMAP`:

```toml
# Pickers (`prefix p` prefix — Helix space-mode mirror).
# `p` is the codon mnemonic for "pickers"; the global chord
# namespace makes these reachable from terminal / file-manager
# panes too, not just editors.
"prefix p f"       = "file_finder::Toggle"
"prefix p b"       = "tab_switcher::Toggle"
"prefix p s"       = "outline::Toggle"
"prefix p shift-s" = "project_symbols::Toggle"
"prefix p d"       = "diagnostics::Deploy"
"prefix p shift-d" = "diagnostics::Deploy"      # workspace variant — replace with project-wide action if Zed exposes one
"prefix p r"       = "projects::OpenRecent"
```

Verify each action name is registered:

- `file_finder::Toggle` — `vendor/zed/crates/file_finder/`.
- `tab_switcher::Toggle` — `vendor/zed/crates/tab_switcher/`.
- `outline::Toggle` — `vendor/zed/crates/outline/`.
- `project_symbols::Toggle` —
  `vendor/zed/crates/project_symbols/`.
- `diagnostics::Deploy` — `vendor/zed/crates/diagnostics/`.
- `projects::OpenRecent` — `vendor/zed/crates/recent_projects/`
  (the action namespace in Zed may be `projects` or
  `recent_projects`; verify before binding).

The `prefix` sentinel is expanded by codon-keymap's loader
([`load_codon_keymap`](spec:src:crates/codon-keymap/src/keymap.rs))
into whatever the user has configured (default `cmd-k`).

## Why this clause

Helix users land in codon and reach for `space f`, `space b`,
`space s`. The codon answer is "`space` is editor-mode only; the
global chord prefix is what you want." Giving them a `prefix p`
namespace that mirrors Helix's space-mode pickers preserves the
muscle memory without giving up the editor's `space` namespace
for visual/select moves.

`prefix p` is the codon-style aliasing. The exact letter map is
chosen to match Helix:

| Helix | Codon         | Action                       |
| ----- | ------------- | ---------------------------- |
| space f | prefix p f  | file picker                  |
| space b | prefix p b  | buffer / tab switcher        |
| space s | prefix p s  | document symbols (outline)   |
| space S | prefix p ⇧S | workspace symbols            |
| space d | prefix p d  | diagnostics                  |
| space D | prefix p ⇧D | workspace diagnostics        |
| space r | prefix p r  | recent projects              |

The remaining Helix space verbs (`space j` jumplist, `space g`
changed files, `space '` last picker) need new picker delegates;
see [`phase-16/pickers-jumplist`](spec:.specs/phase-16/pickers-jumplist.spec.md),
[`phase-16/pickers-changed-files`](spec:.specs/phase-16/pickers-changed-files.spec.md),
[`phase-16/pickers-last-picker`](spec:.specs/phase-16/pickers-last-picker.spec.md).

## Verification

- Press `cmd-k p f` from any pane (editor, terminal, file
  manager). File finder opens.
- Press `cmd-k p b`. Tab switcher opens.
- Cheatsheet renders the new bindings under the global tab.
- `cargo test -p codon-keymap` passes; the binding-count assertion
  from `phase-16/helix-bindings-mirror` accounts for the added
  global entries.

## Done when

- `DEFAULT_KEYMAP` contains the seven `prefix p <letter>`
  bindings.
- All seven actions are registered (no `cannot build action`
  warnings at boot).
- The cheatsheet renders the bindings.
- `spec lint` is at zero errors.
