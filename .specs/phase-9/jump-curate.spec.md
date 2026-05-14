---
id: TASK:phase-9/jump-curate
type: task
status: accepted
version: 0.0.1
summary: >
  Add `g w` / `g W` entries to codon-keymap's `[bindings.editor.normal]`
  defaults and resolver arms for `vim::HelixJumpToWord` /
  `vim::HelixExtendToWord` so the upstream feature appears in the
  curated cheatsheet.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/editor-jumps#c-jump-to-word-curated
  - REQ:codon/editor-jumps#c-jump-extend-curated
  - REQ:codon/editor-jumps#c-resolver-arms
aspects: [gw-toml, gW-toml, resolver-arms]
---

# Surface Helix jump-to-word in the curated cheatsheet

## What ships

Two lines under `[bindings.editor.normal]` in
`crates/codon-keymap/src/keymap.rs::DEFAULT_KEYMAP`:

```toml
"g w" = "vim::HelixJumpToWord"
"shift-g shift-w" = "vim::HelixExtendToWord"
```

(`shift-g shift-w` mirrors `vim.json`'s spelling for the
extend chord — keep it identical so chord parsing matches the
upstream registration.)

Plus two resolver arms in `resolve_binding`:

```rust
"vim::HelixJumpToWord" => bind!(vim::HelixJumpToWord),
"vim::HelixExtendToWord" => bind!(vim::HelixExtendToWord),
```

The cheatsheet filter (`collect_bindings`) already keeps every
codon-curated `(chord, action)` tuple, so adding the entries makes
them visible to `cmd-k F1` automatically.

## Verification

- `cargo run -p codon`, open a file, press `g w`: two-letter
  overlay labels render across the visible region, typing two
  matching characters jumps the cursor.
- `cmd-k F1`: cheatsheet shows the `g w` and `shift-g shift-w`
  entries under the editor section.
- Override in `~/.config/codon/codon.toml`:
  `"alt-j" = "vim::HelixJumpToWord"` — the resolver arm picks it up
  without requiring the user to touch `vim.json`.

## Where it slots in

[`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
— two TOML lines + two resolver arms. ~10 LOC.
