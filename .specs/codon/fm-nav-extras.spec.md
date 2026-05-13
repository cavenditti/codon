---
id: REQ:codon/fm-nav-extras
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  File manager navigation beyond hjkl — directory history, goto-by-path,
  reveal-by-path, and persistent vi-style bookmarks.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-6]
---

# File manager navigation extras

:::{requirement id="fm-nav-extras" level="SHOULD"}
The file manager SHOULD support:

- {#c-history-back-forward} a directory-history stack — `[` /
  `ctrl-o` step back, `]` / `ctrl-i` step forward. Replaces the
  current "go up one level" semantics on `h` only when the user
  pressed something other than `h` to enter the dir.
- {#c-goto-path} `:cd <path>` input prompt for absolute, relative
  or `~`-prefixed paths; tab-completes against the filesystem.
- {#c-reveal-file} a `codon_fm::Reveal(PathBuf)` action that sets
  `current_dir` to the path's parent and selects the entry. Callable
  from anywhere (project picker, command palette, agent output).
- {#c-bookmarks} `m<letter>` saves the current `current_dir` to the
  letter's slot; `'<letter>` jumps there. 26 slots, persisted globally
  to `~/.local/state/codon/fm-bookmarks.toml` so they survive
  restarts and follow the user across all codon launches.
:::
