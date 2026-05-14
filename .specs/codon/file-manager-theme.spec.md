---
id: REQ:codon/file-manager-theme
type: requirement
status: accepted
version: 1.0.0
level: SHOULD
summary: >
  Yazi-style visual polish for the file manager — per-filetype
  filename colors, stronger git-status tint, footer mode badge,
  marked-row stripe, cursor-row contrast bump, header sort/filter
  chips.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-9]
---

# File manager theme

The file manager currently inherits Zed's neutral panel palette.
Yazi-faithful behavior is mostly in place; what's missing is the
visual rhythm that makes a column scan-able at a glance — colored
filenames per type, conspicuous git-status, an unmistakable
marked-vs-cursor distinction, and a mode badge mirroring the
codon-mode tracker.

All colors come from codon-mode theme tokens or from a small
TOML overlay at `~/.config/codon/file-manager-theme.toml`. No
ad-hoc constants in the renderer.

:::{requirement id="file-manager-theme" level="SHOULD"}
The file manager SHOULD:

- {#c-filetype-colors} colorize filename text by extension using
  a built-in palette (sources: rust orange, markdown blue,
  json/yaml cyan, images magenta, archives yellow, executables
  green-bold, configs muted, dotfiles dim). Loaded from an
  embedded TOML default and overridable by
  `~/.config/codon/file-manager-theme.toml`'s `[filetype]`
  table. Reload on file change via the same `Fs::watch` channel
  the openers config uses.
- {#c-git-status-colors} replace the current low-contrast git
  decoration with stronger tints — staged green-bold, modified
  yellow-bold, deleted red, untracked cyan, conflicted magenta.
  Tints apply to both the filename and the leading status glyph
  (`M`/`A`/`D`/`U`/`!`).
- {#c-mode-badge} render a small colored badge in the file
  manager footer reflecting the current `CodonModeTracker` state
  (Normal green, Insert blue, Visual orange). Updates live as
  focus shifts between fm modal prompts and the row list.
- {#c-marked-row-stripe} marked rows get a 2px left-edge stripe
  in the accent color (in addition to the existing background
  tint) so marked-but-not-current rows stay visible when the
  cursor moves away.
- {#c-cursor-row-contrast} bump the cursor-row background from
  the current low-alpha overlay to a theme-defined
  `ghost_element_active` tint, and bold the filename. The row
  must remain readable when marked + cursor coincide.
- {#c-header-chips} the column header shows compact colored
  chips for active modifiers — sort mode (`name`, `size`,
  `mtime`, `btime`, `ext`, `nat`, `rand`) and direction arrow,
  filter active (yellow `filter:<pattern>`), find active
  (cyan `find:<pattern>` with match count), hidden visible
  (`.` muted). Chips disappear when their state is default.
:::
