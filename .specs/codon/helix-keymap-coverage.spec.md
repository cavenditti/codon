---
id: REQ:codon/helix-keymap-coverage
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Mirror the helix-mode editor bindings from vendored Zed's
  `vim.json` into codon's embedded `DEFAULT_KEYMAP` under
  `[bindings.editor.normal]` so the `cmd-k F1` cheatsheet renders
  them and so users have a single source of truth for editor
  shortcuts (the codon TOML). Add the small set of Helix verbs whose
  backing actions exist in Zed but aren't bound under helix-mode
  (`q`/`Q` macros, `(`/`)` rotate selections, `&` align in columns,
  `g f` goto file, `g d`/`g i`/`g y` definitions).
owners: [carlo]
categorized_under: [TOPIC:topics/phase-16]
---

# Helix keymap coverage in the codon cheatsheet

## Context

Codon's cheatsheet
([`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs))
reads `codon_default_bindings()` /
`codon_user_bindings()` — both of which parse the embedded
`DEFAULT_KEYMAP` and `~/.config/codon/codon.toml` respectively. Any
binding declared elsewhere (Zed's JSON keymaps, vendored
`vim.json`) is invisible to it. That's by design — the alternative
of surfacing all ~1000 Zed defaults would drown the codon-specific
verbs in noise.

The consequence is that the helix-mode block in
[`vendor/zed/assets/keymaps/vim.json:421-551`](spec:src:vendor/zed/assets/keymaps/vim.json)
— ~70 bindings under `vim_mode == helix_normal || helix_select` —
never appears in `cmd-k F1`. Users coming from Helix can't discover
the editor-side keymap from the codon UI even though codon binds it.

The same audit
([conversation summary, 2026-05-18](spec:.specs/codon/helix-keymap-coverage.spec.md))
turned up a small set of Helix verbs whose backing Zed actions
exist but aren't bound under helix-mode:

- `q` / `Q` — `vim::ToggleRecord` / `vim::ReplayLastRecording`
  (bound only under `vim_mode == normal` at vim.json:139-140).
- `(` / `)` — `editor::RotateSelectionsBackward` /
  `editor::RotateSelectionsForward` (live in `editor.rs:12806-12815`,
  unbound; `(` / `)` are free in helix-normal).
- `&` — `editor::AlignSelections` (live in `editor.rs:12579`).
- `g f` — `editor::OpenSelectedFilename`.
- `g d`, `g i`, `g y` — `editor::GoToDefinition` /
  `GoToImplementation` / `GoToTypeDefinition`. `g r` is bound in
  the helix block; the other three only inherit from Zed defaults
  under non-helix contexts.

Codon already owns the convention that TOML is the single source of
configuration
([memory: feedback_toml_single_source](spec:memory/feedback_toml_single_source.md)).
Mirroring the helix-mode bindings into `DEFAULT_KEYMAP` upholds it
and fixes the cheatsheet at the same time.

:::{requirement id="helix-keymap-coverage" level="SHOULD"}
The codon keymap default SHOULD:

- {#c-mirror-vim-json-helix} mirror every binding in the
  `vim_mode == helix_normal || helix_select` block of
  `vendor/zed/assets/keymaps/vim.json` under
  `[bindings.editor.normal]` of `DEFAULT_KEYMAP`, preserving
  semantics. Mirrored bindings re-register against the same action
  and same predicate, so the live behaviour does not change — only
  the cheatsheet's surface does.
- {#c-macro-bindings} bind `q` to `vim::ToggleRecord` and `shift-q`
  to `vim::ReplayLastRecording` under helix-normal context.
- {#c-rotate-selections} bind `(` to
  `editor::RotateSelectionsBackward` and `)` to
  `editor::RotateSelectionsForward` under helix-normal context.
- {#c-align-selections} bind `&` to `editor::AlignSelections` under
  helix-normal context.
- {#c-goto-extra} bind `g f` to `editor::OpenSelectedFilename`,
  `g d` to `editor::GoToDefinition`, `g i` to
  `editor::GoToImplementation`, and `g y` to
  `editor::GoToTypeDefinition` under helix-normal context.
- {#c-cheatsheet-renders} render every mirrored binding in the
  `cmd-k F1` cheatsheet, grouped under an "Editor (Helix)" tab or
  the existing editor tab — whichever the cheatsheet's tab model
  already supports without churn.
- {#c-no-binding-leak} not introduce any binding that fires outside
  the helix-normal / helix-select context (avoid stomping vim mode
  or non-editor surfaces).
:::

## Why this REQ

The cheatsheet is codon's discoverability surface. Hiding ~70 Helix
bindings behind a vendored JSON file users will never read makes
codon look thinner than it is. The mirror is also cheap — TOML
edits, no Rust changes — and it lines up with the project
invariant that TOML is the single source.

## Done when

- `DEFAULT_KEYMAP` contains an `[bindings.editor.normal]` block with
  every binding from the helix-normal / helix-select section of
  `vim.json`.
- The five "missing-but-bindable" verbs (`q`/`Q`, `(`/`)`, `&`,
  `g f`, `g d`/`g i`/`g y`) are present.
- `cmd-k F1` lists them.
- `spec lint` is at zero errors.
- `vendor/zed/script/clippy` (when editing vendored Zed) or
  `cargo clippy -p codon-keymap` reports no new warnings.
