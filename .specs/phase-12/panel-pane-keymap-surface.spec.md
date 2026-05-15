---
id: TASK:phase-12/panel-pane-keymap-surface
type: task
status: accepted
version: 0.0.1
summary: >
  Two codon-namespaced actions per converted panel — Open<Name>
  (default pane) and Peek<Name> (transient dock) — plus default
  cmd-k chord bindings and example codon.toml entries.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/panes-from-panels#c-keymap-surface
---

# Codon-namespaced open / peek actions per panel

## What ships

Two GPUI actions per converted panel, registered in `codon-panes`:

```rust
codon_panes::OpenAgent       codon_panes::PeekAgent
codon_panes::OpenGit         codon_panes::PeekGit
codon_panes::OpenOutline     codon_panes::PeekOutline
codon_panes::OpenDebug       codon_panes::PeekDebug
```

Default keymap in the embedded codon TOML (chord layout subject to
review during prototyping):

| Chord | Action |
|---|---|
| `cmd-k a` | `codon_panes::OpenAgent` |
| `cmd-k shift-a` | `codon_panes::PeekAgent` |
| `cmd-k g` | `codon_panes::OpenGit` |
| `cmd-k shift-g` | `codon_panes::PeekGit` |
| `cmd-k o` | `codon_panes::OpenOutline` |
| `cmd-k shift-o` | `codon_panes::PeekOutline` |
| `cmd-k d` | `codon_panes::OpenDebug` |
| `cmd-k shift-d` | `codon_panes::PeekDebug` |

`cmd-k g s` (today bound to `git_panel::ToggleFocus`) is rebound to
`codon_panes::OpenGit`. The legacy `*_panel::ToggleFocus` actions
are no longer the codon entry point — they remain bound by Zed for
internal use but the codon keymap stops referencing them.

## Approach

1. Action registrations and dispatch: each `Open*` builds (or
   reuses) the panel entity and inserts the adapter into the active
   pane via `Workspace::split` / `Workspace::open_item`-style API.
   Each `Peek*` builds (or reuses) the entity and hands it to
   `peek_panel` with the panel's preferred side.
2. Panel entity lifetime: panels are workspace-scoped singletons.
   Re-invoking `OpenAgent` while the adapter is already mounted in
   a pane focuses that pane (matches existing terminal /
   file-manager open-or-focus behaviour). If the panel is currently
   peeked, the peek closes and the panel re-mounts as a pane.
3. Keymap TOML changes:
   - Embedded default in
     [`crates/codon-keymap/src/keymap.rs`](spec:src:crates/codon-keymap/src/keymap.rs)
     gains the eight new chords above.
   - `assets/config/codon.example.toml` mirrors the new section
     (commented out, with a one-line description of each chord).
   - The `cmd-k g s` rebind retires
     `codon_git::OpenStatusPane` / `git_panel::ToggleFocus` as the
     codon-side action.
4. Action resolution: `resolve_binding` in codon-keymap gains
   arms for the eight new identifiers.

## Non-goals

- Per-user peek-side override (left/right/bottom) — out of scope
  for v1. The preferred side per panel is hard-coded in
  `panel-inventory-decision`.
- Cheatsheet integration — handled by the existing cmd-k F1 modal
  picking these up automatically once they're bound.

## Files touched

- `crates/codon-panes/src/actions.rs` (new) — action structs and
  dispatch closures.
- `crates/codon-keymap/src/keymap.rs` — embedded default TOML,
  resolve_binding arms.
- `assets/config/codon.example.toml` — new commented block.
