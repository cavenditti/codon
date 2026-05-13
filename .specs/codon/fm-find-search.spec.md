---
id: REQ:codon/fm-find-search
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Find / filter / search verbs — `/` and `?` jump-to-match,
  `f` filter-by-hide, `s` external name search (fd), `S` external
  content search (ripgrep), `z` zoxide jump.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-7]
---

# File manager find and search

Codon's existing `/` is filter (hides non-matching entries). Yazi
splits that into find (`/` jumps to first match, `n` / `N` walk
matches) and filter (`f` hides non-matches). Adopting both is a
strict superset of today's behavior: `/` becomes find-forward,
`?` becomes find-backward, and the current `/` filter migrates to
`f`. `n` / `N` are repeat-search-forward / -backward in the
established vim style.

:::{requirement id="fm-find-search" level="SHOULD"}
The file manager SHOULD support:

- {#c-find-mode} `/` opens an Insert-mode prompt; on each
  keystroke the cursor jumps to the first entry whose name
  contains the substring (case-insensitive). `Enter` commits the
  query as the last-find-pattern; `Esc` cancels. `n` / `N` after
  commit walk forward / backward through matches. `?` is the same
  prompt but the initial walk direction is backward.
- {#c-filter-rebind} `f` opens the existing fuzzy-filter prompt
  (today's `/` behavior moves here verbatim — the implementation
  and Insert-mode commit path are unchanged).
- {#c-search-by-name} `s` invokes `fd` if installed (else
  `walkdir` as fallback) starting at `current_dir`. Results open
  in a `Picker` modal; Enter reveals the chosen path via
  `codon_fm::Reveal`.
- {#c-search-by-content} `S` invokes `ripgrep` starting at
  `current_dir`; the picker shows `path:line: snippet`. Enter
  opens the file at the line via `workspace.open_abs_path` with a
  position hint. If `ripgrep` is not installed, surface a toast
  with installation guidance — no walkdir fallback (would be too
  slow without ranking).
- {#c-zoxide-jump} `z` invokes `zoxide query -l` if installed;
  the picker fuzzy-filters the returned paths. Enter sets
  `current_dir` to the choice. No-op + toast when zoxide is
  missing.
:::
