---
id: REQ:codon/helix-pickers
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Bind the Helix space-mode picker family (file / buffer / symbols
  / diagnostics / recent) under a codon `prefix p <letter>` chord
  namespace, add three new pickers Zed doesn't ship (jumplist,
  changed-files, last-picker), and surface every picker through
  the codon cheatsheet. Distinct in scope from
  [`REQ:codon/in-app-pickers`](spec:.specs/codon/in-app-pickers.spec.md),
  which is about replacing OS-native file dialogs with picker
  delegates — this REQ is about *bindings* and *new* picker
  delegates for Helix muscle memory.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-16]
---

# Helix-style picker namespace + new pickers

## Context

Helix exposes pickers under its `space` minor mode:

- `space f` — file picker (workspace root).
- `space F` — file picker (cwd).
- `space b` — buffer picker.
- `space j` — jumplist picker.
- `space g` — changed-file picker (git status).
- `space s` / `space S` — document / workspace symbols.
- `space d` / `space D` — document / workspace diagnostics.
- `space r` — rename symbol (not a picker).
- `space '` — reopen the last picker.

Vendored Zed already implements most of these as separate actions
(`file_finder::Toggle`, `tab_switcher::Toggle`, `outline::Toggle`,
`project_symbols::Toggle`, `diagnostics::Deploy`,
`projects::OpenRecent`). What's missing:

1. **No unified Helix-style namespace.** `space` is owned by the
   helix-mode editor context, not by codon's global chord prefix.
   Surface the same picker family under `prefix p <letter>` (the
   codon chord prefix is configurable; `p` = pickers) so non-editor
   panes can reach pickers with the same muscle memory.
2. **No jumplist picker.** Helix shows the jumplist as a picker
   over recent cursor positions. Zed has a jumplist concept
   internally (`editor::OpenSelectionsInMultiBuffer`-adjacent), but
   no picker that lists it. Codon also keeps a pane-history stash
   in `codon-session::runtime::WindowRuntimeCache`; combining the
   two into a single "places I've been" picker is the natural fit.
3. **No changed-files picker.** Codon's git pane shows changed
   files in a list, but there's no quick-open picker form. Wrap
   git status into a `picker::Picker` delegate so `prefix p g`
   jumps directly to a changed file by fuzzy match.
4. **No "reopen last picker".** Helix's `space '` reopens the most
   recently dismissed picker with its query intact. A small
   workspace-level singleton stash plus a `LastPicker` action
   covers it.

:::{requirement id="helix-pickers" level="SHOULD"}
The picker family SHOULD:

- {#c-prefix-namespace} expose a `prefix p <letter>` chord namespace
  binding the Helix space-mode pickers to existing Zed actions:
  - `prefix p f` → `file_finder::Toggle`
  - `prefix p b` → `tab_switcher::Toggle`
  - `prefix p s` → `outline::Toggle`
  - `prefix p shift-s` → `project_symbols::Toggle`
  - `prefix p d` → `diagnostics::Deploy`
  - `prefix p shift-d` → `diagnostics::Deploy` (workspace variant —
    or a separate action if Zed exposes one)
  - `prefix p r` → `projects::OpenRecent`
- {#c-jumplist-picker} provide a `codon_pickers::JumplistPicker`
  action that opens a `picker::Picker` over the active editor's
  jumplist (vim crate's `JumpList`) joined with codon's recent
  pane-activation history. Bound under `prefix p j`.
- {#c-changed-files-picker} provide a
  `codon_pickers::ChangedFilesPicker` action that opens a
  `picker::Picker` over the project's git status, restricted to
  files with non-`Unmodified` status. Bound under `prefix p g`.
  Confirming a row opens the file at the first changed hunk.
- {#c-last-picker} track the most recently dismissed picker in a
  workspace-scoped singleton and add a
  `codon_pickers::LastPicker` action that reopens it with the
  prior query. Bound under `prefix p '`.
- {#c-cheatsheet-coverage} surface every binding above in the
  `cmd-k F1` cheatsheet (it already reads codon TOML — adding the
  bindings to `DEFAULT_KEYMAP` is sufficient).
- {#c-respect-pane-context} fire from any pane context, not just
  editors. The `prefix p` chord namespace lives under
  `[bindings.global]`, not `[bindings.editor.normal]`.
:::

## Why this REQ

Helix users land in codon expecting `space f` to open a file
picker. Codon's choice was to keep `space` for editor mode, but
binding the same set under the global chord prefix gives the
muscle memory back without giving up the editor-mode `space`
namespace. The three new pickers (jumplist, changed-files, last)
are the only ones that need real implementation work — everything
else is bindings.

## Done when

- `DEFAULT_KEYMAP` carries the `prefix p <letter>` binding family.
- A `codon-pickers` extension or a new crate hosts
  `JumplistPicker`, `ChangedFilesPicker`, and `LastPicker` action
  handlers + their `picker::Picker` delegates.
- The cheatsheet renders every binding.
- `spec lint` is at zero errors.
- `cargo clippy` reports no warnings.
