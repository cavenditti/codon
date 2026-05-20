---
id: TASK:phase-20/verb-collapse-close
type: task
status: draft
version: 0.0.1
summary: >
  Rename `codon_session::SafeCloseActiveItem` to `codon_session::Close`
  (or `codon::Close`) and make it the single user-facing close verb
  across panes. Drop `pane::CloseActiveItem` from the Helix mirror
  block in the embedded defaults (`space w q` rebinds to the codon
  cascade). Add an optional `codon_session::CloseForce` action for
  the rare bypass case; do not bind it by default.
owners: [carlo]
progress: done
refines:
  - REQ:codon/keymap-vocabulary#c-verb-collapse-close
---

# Verb collapse — close

## Plan

Today's close surface has two paths:

- `codon_session::SafeCloseActiveItem` — smart cascade
  (close item → close pane → close session window → empty pane).
  Bound to `cmd-w` and (historically) `prefix w`.
- `pane::CloseActiveItem` — raw Zed action that just closes the
  active item. Bound on the Helix mirror as `space w q`.

The two paths quietly disagree: a Helix user pressing `space w q`
gets *different* close semantics than a Mac user pressing `cmd-w`.
Phase 20 consolidates to one verb.

### What ships

1. **Rename** `codon_session::SafeCloseActiveItem` →
   `codon_session::Close` in
   [`crates/codon-session/`](spec:src:crates/codon-session/).
   Keep `SafeCloseActiveItem` as a deprecated alias for one release
   cycle so user keymap overrides referencing the old name still
   parse (log a deprecation hint at bind time).

2. **Rebind embedded defaults**:

   ```toml
   # Single close verb across pane kinds.
   "cmd-w"     = "codon_session::Close"
   # Helix mirror block — was `pane::CloseActiveItem`.
   "space w q" = "codon_session::Close"
   ```

   No `prefix`-based close chord is added — `cmd-w` is the only
   short-chord path. `prefix w` is now the window-verbs sub-prefix
   per [TASK:phase-20/keymap-chord-rename](spec:TASK:phase-20/keymap-chord-rename).

3. **Optional** `codon_session::CloseForce` — bypasses the cascade
   and just closes the active item even when codon would otherwise
   cascade to close the pane / window. Unbound by default. Add only
   if the implementation diff is small; otherwise defer to a
   follow-up.

4. **Drop** any other `pane::CloseActiveItem` reference from the
   embedded defaults if present (none expected after the `space w q`
   rebind; double-check `crates/codon-keymap/src/keymap.rs`).

### Notes

- The user's existing `~/.config/codon/codon.toml` is not touched.
  Overrides that bind `codon_session::SafeCloseActiveItem` continue
  to work via the deprecated alias.
- The Mac `cmd-w` convention is preserved as the close chord.
  Tmux's `prefix w = window list` convention is restored by
  [TASK:phase-20/keymap-chord-rename](spec:TASK:phase-20/keymap-chord-rename).

## Acceptance

- `cmd-w` and `space w q` produce identical cascade behaviour
  across editor / terminal / file-manager / git-panel.
- Loading a user config that references
  `codon_session::SafeCloseActiveItem` still binds, with a single
  deprecation log line.
- `cargo test -p codon-session` passes; tests cover the cascade
  for at least one item-close, one pane-close, and one
  window-close case.
- `spec lint` clean.

## Files touched

- `crates/codon-session/src/safe_close.rs` (rename / add alias).
- `crates/codon-keymap/src/keymap.rs` — embedded defaults rebind.
- `assets/config/codon.example.toml` — comment block reflects the
  cmd-w / space w q convergence.
- Tests in `crates/codon-session/`.
