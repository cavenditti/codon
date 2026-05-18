---
id: TASK:phase-16/helix-bindings-mirror
type: task
status: draft
version: 0.0.1
summary: >
  Append the helix-mode editor bindings from
  `vendor/zed/assets/keymaps/vim.json` into codon's embedded
  `DEFAULT_KEYMAP` under `[bindings.editor.normal]`, plus the five
  "bindable-now" gaps (`q`/`Q`, `(`/`)`, `&`, `g f`, `g d`/`g i`/`g y`).
  The cheatsheet starts surfacing them automatically.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/helix-keymap-coverage#c-mirror-vim-json-helix
  - REQ:codon/helix-keymap-coverage#c-macro-bindings
  - REQ:codon/helix-keymap-coverage#c-rotate-selections
  - REQ:codon/helix-keymap-coverage#c-align-selections
  - REQ:codon/helix-keymap-coverage#c-goto-extra
  - REQ:codon/helix-keymap-coverage#c-cheatsheet-renders
  - REQ:codon/helix-keymap-coverage#c-no-binding-leak
aspects: [mirror-vim-json, macros, rotate, align, goto-extra, cheatsheet, predicate-safety]
---

# Mirror helix-mode bindings into codon TOML defaults

## What changes

[`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
holds `DEFAULT_KEYMAP: &str`. Today its `[bindings.editor.normal]`
section is short (~6 entries: the `:` palette redirect plus
`g w` jump-to-word). Extend it to mirror the helix-mode block from
[`vendor/zed/assets/keymaps/vim.json:421-551`](spec:src:vendor/zed/assets/keymaps/vim.json).

The mirror is mechanical:

1. Read the binding pairs from the vim.json block(s):
   - The `(vim_mode == helix_normal || helix_select)` block at
     lines 445-551.
   - The `vim_mode == helix_normal` block at lines 421-437
     (insert-mode entries, escape, motion variants).
2. Re-encode them as TOML `"keystroke" = "namespace::Action"`
   entries under `[bindings.editor.normal]` in codon's
   `DEFAULT_KEYMAP`.
3. Action payloads (`["vim::Down", { "display_lines": true }]`)
   become TOML-encoded JSON args via the existing parenthesised
   action-spec form codon-keymap already supports
   (`parse_action_spec` at `keymap.rs:588-600`):
   `"j" = "vim::Down({\"display_lines\":true})"`.

Then add the five gaps:

```toml
[bindings.editor.normal]
"q"        = "vim::ToggleRecord"
"shift-q"  = "vim::ReplayLastRecording"
"("        = "editor::RotateSelectionsBackward"
")"        = "editor::RotateSelectionsForward"
"&"        = "editor::AlignSelections"
"g f"      = "editor::OpenSelectedFilename"
"g d"      = "editor::GoToDefinition"
"g i"      = "editor::GoToImplementation"
"g y"      = "editor::GoToTypeDefinition"
```

The existing `mode_predicates` mapping
([`keymap.rs:456-473`](spec:src:crates/codon-keymap/src/keymap.rs))
already resolves `Editor` to
`vim_mode == normal || vim_mode == helix_normal || vim_mode == helix_select`,
so every mirrored binding gets the right predicate automatically.

## Why this clause

Re-binding the same chord to the same action under the same
predicate is a no-op for GPUI (the binding table is keyed on
predicate + chord), but `codon_default_bindings()` parses the TOML
fresh each time the cheatsheet opens and surfaces every entry.
Mirroring fixes discoverability at near-zero cost.

## Verification

- Open codon. Press `cmd-k F1`. The editor tab should now list the
  helix-mode chord family (`d`, `c`, `y`, `p`, `s`, `;`, `,`, `x`,
  `m m`, `m s`, `]`, `[`, `g e`, `g h`, `space w …`, `space f`,
  `space d`, etc.) instead of just `:` and `g w`.
- Press `q` in helix-normal mode → recording-state indicator
  appears (vim crate already wires the action; only the binding
  is new).
- Press `(` / `)` in helix-normal with multiple cursors → primary
  cursor rotates backward / forward through the set.
- Press `&` with column-staggered cursors → cursors align.
- Press `g f` over a visible filename → opens the file.
- `cargo test -p codon-keymap` passes; existing tests aren't
  affected (mirror is additive).
- `spec lint` reports zero errors.

## Done when

- `[bindings.editor.normal]` in `DEFAULT_KEYMAP` contains every
  helix-mode binding from vim.json plus the five gap verbs.
- The cheatsheet renders them.
- A new test
  `mirrored_helix_bindings_appear_in_default_bindings` asserts the
  binding count meets a lower bound (e.g., ≥ 60 entries) so
  regressions on the mirror are caught.
- `vendor/zed/script/clippy` reports no new warnings.
- `spec lint` is at zero errors.
