---
id: TOPIC:topics/phase-16
type: topic
status: draft
version: 0.0.1
summary: >
  Helix UX coverage — duplicate the helix-mode editor bindings into
  codon's TOML defaults so the cheatsheet sees them, replace Zed's
  small bottom-right which-key panel with a full-width codon overlay
  that auto-flips to the top when it would occlude the active pane,
  add Helix-style pickers (file/buffer/symbols/diagnostics + a
  jumplist picker, a changed-files picker, and a `last picker`
  action), and wire Helix's shell verbs (`|` `!` `$` `Alt-|` `Alt-!`)
  with a matching `:sh` / `:pipe` palette path.
owners: [carlo]
---

# Phase 16 — Helix UX coverage

Codon's editor side already inherits ~80% of Helix's normal/select
mode from vendored Zed's `vim.json` (`helix_normal || helix_select`
block at `vendor/zed/assets/keymaps/vim.json:421-551`). The four
visible gaps users hit in practice are:

1. **The cheatsheet doesn't surface any of it.** `cmd-k F1` filters
   the global GPUI binding registry down to codon's curated TOML, so
   the ~70 helix-mode bindings that vim.json wires are invisible to
   anyone browsing for them. The shape that fits codon convention is
   to mirror those bindings into `DEFAULT_KEYMAP` under
   `[bindings.editor.normal]`. Re-binding the same chord to the same
   action is a no-op for GPUI, and the cheatsheet picks them up.

2. **Zed's which-key panel is small and bottom-right.**
   `vendor/zed/crates/which_key/src/which_key_modal.rs:238-256`
   renders a max-480-px wide, max-40 %-tall floating panel pinned to
   the bottom-right corner. Helix's chord HUD sits across the full
   width of the terminal at the bottom (or top of the pane when the
   pane is short). Replace the renderer with a codon variant that
   uses the active pane's bounds, auto-flips to the top, and
   multi-columns its content so it never out-scrolls.

3. **Pickers are scattered.** Zed has a file finder, a tab switcher,
   an outline, a project-symbols picker, and a diagnostics picker —
   but no Helix-style namespace (`space f`, `space b`, `space s`, …)
   and no jumplist picker, no changed-files picker, no "reopen the
   last picker" action. Codon already owns `cmd-k`-prefixed chords;
   add a `prefix p <letter>` family that mirrors Helix's space-mode
   pickers, plus three new picker delegates for jumplist /
   changed-files / last-picker.

4. **Shell verbs don't exist.** Helix's `|` / `!` / `$` / `Alt-|` /
   `Alt-!` pipe-selection-through-shell verbs are Helix's signature
   feature and codon is the natural home (it owns terminal panes).
   Add five `vim::Shell*` actions, a 1-line prompt over
   `codon-pickers::ModalScaffold`, and a matching palette verb set
   (`:sh`, `:pipe`, `:insert-output`, `:append-output`,
   `:keep-pipe`) via `codon-command-palette::completer`.

Phase 16 closes those four gaps in four REQs:

- [`REQ:codon/helix-keymap-coverage`](spec:.specs/codon/helix-keymap-coverage.spec.md) — the mirror.
- [`REQ:codon/which-key-overlay`](spec:.specs/codon/which-key-overlay.spec.md) — the HUD.
- [`REQ:codon/helix-pickers`](spec:.specs/codon/helix-pickers.spec.md) — the pickers.
- [`REQ:codon/shell-integration`](spec:.specs/codon/shell-integration.spec.md) — the shell.

Phase 16 ships when every TASK under those four REQs is `done` and
`spec lint` is at zero errors.
