---
id: TOPIC:topics/phase-7
type: topic
status: draft
version: 0.0.1
summary: >
  Yazi-feature-parity for the file manager, wave 2 — find/search,
  openers, shell exec.
owners: [carlo]
---

# Phase 7 — File-manager parity, wave 2

Phase 6 ships everything that's pure UX inside the FM model.
Phase 7 picks up the capabilities that reach outside the file manager —
to the project's indexers (`fd`, `ripgrep`), to user-configured openers,
and to the codon terminal pane for shell exec.

Design pivot on filter / search: codon's existing `/` (filter — hides
non-matches) migrates to `f`. `/` becomes search-forward (find mode,
jumps to first match, `n` / `N` cycle); `?` is search-backward. This
matches both vim's chord shapes and yazi's mode names without losing
either capability.

Refining requirements:

- [REQ:codon/fm-find-search](spec:REQ:codon/fm-find-search) —
  `/` / `?` find, `f` filter, `s` search-by-name (fd), `S`
  search-by-content (ripgrep), `z` zoxide jump.
- [REQ:codon/fm-openers](spec:REQ:codon/fm-openers) — `O` choose
  opener, `openers.toml` config, default Enter-route consults openers.
- [REQ:codon/fm-shell-exec](spec:REQ:codon/fm-shell-exec) — `!`
  blocking, `;` async with `%s`/`%S`/`%d`/`%D` substitutions, terminal
  reuse (idle terminal if present, otherwise a new one), stderr toast.

## External tools

All optional; the spec calls out the in-process fallback for each:

- `fd` → fall back to `walkdir` for `s` search-by-name.
- `ripgrep` → no built-in fallback for `S`; the action surfaces a
  hint when the binary is missing.
- `zoxide` → `z` is a no-op (with toast) when not installed.
