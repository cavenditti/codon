---
id: TASK:phase-20/verb-collapse-split
type: task
status: draft
version: 0.0.1
summary: >
  Collapse the four pane-kind-specific split actions
  (`codon_session::SplitTerminal{Right,Down}` /
  `…SplitFileManager{Right,Down}`) plus the raw `pane::SplitRight` /
  `pane::SplitDown` into one pair of `codon_session::Split{Right,Down}`
  actions whose new-pane kind is picked from the active pane's focus
  (terminal → terminal, fm → fm, editor → editor). Keep the existing
  chord shapes `\ | - _` stable, but reroute them through the
  collapsed actions.
owners: [carlo]
progress: done
refines:
  - REQ:codon/keymap-vocabulary#c-verb-collapse-split
---

# Verb collapse — split

## Plan

Today's split surface lives in
[`crates/codon-session/src/split.rs`](spec:src:crates/codon-session/src/split.rs)
and exposes four actions:
`SplitTerminalRight`, `SplitTerminalDown`, `SplitFileManagerRight`,
`SplitFileManagerDown`. The embedded defaults
([`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs))
bind them as:

```toml
"prefix \\" = "codon_session::SplitTerminalRight"
"prefix |"  = "codon_session::SplitFileManagerRight"
"prefix -"  = "codon_session::SplitTerminalDown"
"prefix _"  = "codon_session::SplitFileManagerDown"
```

Plus the Helix mirror block adds raw `pane::SplitRight` / `pane::SplitDown`
on `space w v` / `space w s`.

### What ships

1. **New actions** in `codon-session`:
   - `codon_session::SplitRight` — split right, new pane kind picked
     from the active pane's focus (default terminal if focus is
     ambiguous, e.g. an empty pane).
   - `codon_session::SplitDown` — same axis, vertical split.
   - `codon_session::SplitRightOther` / `codon_session::SplitDownOther`
     — split right/down with the *other* kind in the terminal ↔
     file-manager pair, preserving today's `|` / `_` mnemonic
     ("flip to the other primary pane kind"). For editor focus, "other"
     resolves to terminal.

2. **Dispatcher** picks the kind via the active pane's `PaneKind`
   (terminal / file_manager / editor / panel-as-pane). Cwd seeding
   reuses the existing logic in
   [`SplitTerminalRight::cwd_from_active`](spec:src:crates/codon-session/src/split.rs)
   /
   [`SplitFileManagerRight::dir_from_active`](spec:src:crates/codon-session/src/split.rs)
   factored into a shared helper.

3. **Rebind embedded defaults**:

   ```toml
   "prefix \\" = "codon_session::SplitRight"
   "prefix -"  = "codon_session::SplitDown"
   "prefix |"  = "codon_session::SplitRightOther"
   "prefix _"  = "codon_session::SplitDownOther"
   ```

   The Helix-mirror `space w v` / `space w s` rebind to
   `codon_session::SplitRight` / `codon_session::SplitDown` (drops
   `pane::SplitRight` / `pane::SplitDown` from codon's surface).

4. **Retire** `SplitTerminal{Right,Down}` /
   `SplitFileManager{Right,Down}` as user-facing actions. They MAY
   remain as private dispatch targets called by the new actions but
   MUST NOT appear in the embedded defaults' chord table or the
   example config.

### Edge cases

- **Editor focus + `|`** — "other primary kind" is ambiguous (editor
  doesn't have an obvious pair). Resolve to terminal for now; revisit
  if user feedback wants editor-as-default.
- **Empty pane focus** — fall back to terminal for both `\` and `|`.
- **Peek-dock / panel-as-pane focus** — split should fall through to
  the underlying pane kind, but if that's ambiguous (e.g. agent
  panel), default to terminal.

## Acceptance

- `cargo test -p codon-session` passes; new unit tests cover each
  pane-kind dispatch case for `SplitRight` and `SplitRightOther`
  (terminal-focused, fm-focused, editor-focused, empty pane).
- `prefix \ / - / | / _` work end-to-end across all four focus
  contexts (manual smoke through codon binary).
- `space w v` / `space w s` produce the kind-correct split in the
  editor.
- `spec lint` clean.

## Files touched

- `crates/codon-session/src/split.rs` — new dispatcher actions,
  shared cwd helper.
- `crates/codon-keymap/src/keymap.rs` — embedded TOML rewrite.
- `assets/config/codon.example.toml` — comment block reflects the
  new mnemonic.
- Tests in `crates/codon-session/`.
