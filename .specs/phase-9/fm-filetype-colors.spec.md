---
id: TASK:phase-9/fm-filetype-colors
type: task
status: accepted
version: 0.0.1
summary: >
  Per-extension filename colors loaded from an embedded TOML default
  and overridable by `~/.config/codon/file-manager-theme.toml`'s
  `[filetype]` table. FS-watch hot reload.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/file-manager-theme#c-filetype-colors
---

# File-manager filetype colors

## What ships

A `FmThemeStore` global in `crates/file-manager/src/theme.rs`
mirroring `OpenerStore`'s shape — load + watch
`~/.config/codon/file-manager-theme.toml`, fall back to an
embedded default. Lookup keyed on extension (lowercase), with
explicit categories for dotfiles, executable bits, and "directory"
default (the latter overrides the generic Color::Default for `/`-
suffixed rows).

Built-in palette (resolved against `cx.theme().colors()` so dark/
light themes pick up the right shade):

```
Rust (.rs, .toml)          -> orange
Markdown (.md, .mdx)        -> info  (cyan-ish)
JSON/YAML (.json, .yml, .yaml, .toml) -> conflict (yellow)
TS/JS (.ts, .tsx, .js, .jsx) -> warning
Python (.py, .pyi)           -> created (green)
Shell (.sh, .bash, .zsh, .fish) -> success (green-bold)
Images (.png, .jpg, .jpeg, .webp, .svg, .gif) -> hint (magenta-ish)
Archives (.zip, .tar, .gz, .xz, .zst, .bz2, .7z) -> deleted (red-ish)
Configs (.conf, .ini, .cfg) -> muted
Dotfiles                   -> disabled (dim)
Executable bit set         -> success bold
Directories                -> accent
```

User TOML override:

```toml
[filetype]
rs = "warning"
md = "info"
".env" = "muted"        # leading dot = match exact filename, not ext
```

The view-row renderer in `view.rs` calls
`FmThemeStore::color_for(entry, cx)` for the filename label and
passes the resulting `Color` to the existing `Label`.

## Verification

- `cargo run -p codon`, open file manager: filenames colored per
  extension; directories show in accent; dotfiles dim.
- Edit `~/.config/codon/file-manager-theme.toml`: list updates
  within ~1s without restart (FS-watch path matches openers).
- Bad TOML logs a warning, falls back to embedded default —
  doesn't crash the panel.

## Where it slots in

- New: `crates/file-manager/src/theme.rs` (~250 LOC), `assets/config/file-manager-theme.example.toml`.
- Edit: `crates/file-manager/src/lib.rs` (declare module, init in
  `init(fs, cx)`).
- Edit: `crates/file-manager/src/view.rs` row renderer — single
  call to `FmThemeStore::color_for` replaces the constant
  `Color::Default` filename color.
