---
id: TASK:phase-9/jump-curate
type: task
status: accepted
version: 0.0.1
summary: >
  Add `g w` entry to codon-keymap's `[bindings.editor.normal]`
  defaults and a resolver arm for `vim::HelixJumpToWord` so the
  upstream feature appears in the curated cheatsheet. The same
  action handles Visual-mode extend; no separate binding.
owners: [carlo]
progress: done
refines:
  - REQ:codon/editor-jumps#c-jump-to-word-curated
  - REQ:codon/editor-jumps#c-jump-extend-curated
  - REQ:codon/editor-jumps#c-resolver-arms
aspects: [gw-toml, visual-mode-extend, resolver-arm]
---

# Surface Helix jump-to-word in the curated cheatsheet

## What ships

One line under `[bindings.editor.normal]` in
`crates/codon-keymap/src/keymap.rs::DEFAULT_KEYMAP`:

```toml
"g w" = "vim::HelixJumpToWord"
```

In Visual mode the same action extends the selection
(`HelixJumpBehaviour::Extend` is selected internally when
`Vim::mode.is_visual()` is true), so no separate `g W` /
`HelixExtendToWord` binding is needed.

Plus one resolver arm in `resolve_binding`:

```rust
"vim::HelixJumpToWord" => bind!(vim::HelixJumpToWord),
```

The action is declared via `actions!(vim, [..])` inside
`vendor/zed/crates/vim/src/helix.rs` (private module).
A companion commit on the vendor/zed submodule adds
`pub use helix::HelixJumpToWord;` in `vim.rs` so the type
is reachable from this crate.

The cheatsheet filter (`collect_bindings`) already keeps every
codon-curated `(chord, action)` tuple, so adding the entries makes
them visible to `cmd-k F1` automatically.

## Verification

- `cargo run -p codon`, open a file, press `g w`: two-letter
  overlay labels render across the visible region, typing two
  matching characters jumps the cursor.
- `cmd-k F1`: cheatsheet shows the `g w` entry under the
  editor section.
- Override in `~/.config/codon/codon.toml`:
  `"alt-j" = "vim::HelixJumpToWord"` — the resolver arm picks it up
  without requiring the user to touch `vim.json`.

## Where it slots in

- [`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
  — one TOML line + one resolver arm. ~6 LOC.
- `vendor/zed/crates/vim/src/vim.rs` — one `pub use` line
  exposing `helix::HelixJumpToWord`. ~1 LOC, submodule commit.
