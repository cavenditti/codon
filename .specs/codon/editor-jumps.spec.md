---
id: REQ:codon/editor-jumps
type: requirement
status: accepted
version: 1.0.0
level: SHOULD
summary: >
  Surface upstream Helix-mode jump-to-word (`g w` / `g W`) in codon's
  curated cheatsheet so the feature is discoverable. Implementation
  is already provided by `vim::HelixJumpToWord` /
  `vim::HelixExtendToWord` in vendored Zed.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-9]
---

# Editor jumps

`vendor/zed/crates/vim/src/helix.rs` ships a Helix-faithful
jump-to-word with two-letter overlay labels, visible-range scope,
two-keystroke capture, and theme color
`vim_helix_jump_label_foreground`. The upstream keymap
`vendor/zed/assets/keymaps/vim.json` already binds `g w` →
`vim::HelixJumpToWord`. Codon force-enables Helix mode, so the
feature works today — but it does **not** appear in codon's
`cmd-k F1` cheatsheet because that surface filters against codon's
curated TOML defaults.

:::{requirement id="editor-jumps" level="SHOULD"}
The codon keymap MUST:

- {#c-jump-to-word-curated} include `g w` →
  `vim::HelixJumpToWord` in `[bindings.editor.normal]` so it
  appears in the cheatsheet and can be rebound from
  `~/.config/codon/codon.toml`. The same action auto-extends
  the selection when invoked in Visual mode — no separate
  binding is needed.
- {#c-jump-extend-curated} the Visual-mode extend behavior is
  handled inside `HelixJumpToWord` itself (`Vim::mode.is_visual()`
  switches to `HelixJumpBehaviour::Extend`); no separate
  `HelixExtendToWord` action exists upstream and codon does
  not introduce one.
- {#c-resolver-arms} expose a resolver arm for
  `"vim::HelixJumpToWord"` in `resolve_binding` so a user
  override in `codon.toml` works without depending on the
  upstream `vim.json` default.
:::
