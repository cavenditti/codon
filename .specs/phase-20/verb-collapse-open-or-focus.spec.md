---
id: TASK:phase-20/verb-collapse-open-or-focus
type: task
status: draft
version: 0.0.1
summary: >
  Align `prefix t` / `prefix e` with `cmd-t` / `cmd-e` by binding them
  to `codon_session::GotoOrOpen{Terminal,FileManager}` (focus the
  most-recently-active instance if one exists in the session, else
  open a fresh one in the active pane). Introduce `prefix shift-t` /
  `prefix shift-e` as the always-new variants, backed by
  `workspace::NewTerminal` and a new `file_manager::OpenNew` that
  skips the goto-or-open lookup.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/keymap-vocabulary#c-verb-collapse-open-or-focus
---

# Verb collapse — open-or-focus

## Plan

Today's embedded defaults bind:

```toml
"prefix t"   = "workspace::NewTerminal"           # always-new
"prefix e"   = "file_manager::Open"               # always-new
"cmd-t"      = "codon_session::GotoOrOpenTerminal"
"cmd-e"      = "codon_session::GotoOrOpenFileManager"
```

Two flavours of "open a terminal" under similar chords. Phase 20
collapses the surface to one semantic per chord shape:
single-key-leaf = goto-or-open, shifted-leaf = always-new.

### What ships

1. **Rebind embedded defaults**:

   ```toml
   "prefix t"       = "codon_session::GotoOrOpenTerminal"
   "prefix e"       = "codon_session::GotoOrOpenFileManager"
   "prefix shift-t" = "workspace::NewTerminal"
   "prefix shift-e" = "file_manager::OpenNew"   # new action, below
   ```

2. **New action** `file_manager::OpenNew` in
   [`crates/file-manager/src/`](spec:src:crates/file-manager/src) —
   sibling to the existing `file_manager::Open`, but skips the
   "focus existing instance in session" lookup that the
   `GotoOrOpenFileManager` path performs. Behaviourally identical
   to today's `file_manager::Open` for callers that already get a
   fresh pane; the rename is purely to make "always-new" explicit
   in the action name.

   Alternative: keep `file_manager::Open` as always-new and rename
   the existing call sites that expect goto-or-open to use
   `GotoOrOpenFileManager` directly. Either shape is acceptable —
   pick the one that minimises call-site churn at implementation
   time and document the choice in the merge commit.

3. **Update** the example config
   ([`assets/config/codon.example.toml`](spec:src:assets/config/codon.example.toml))
   to reflect the new chord scheme and add a comment block
   explaining the `t` vs `shift-t` semantic.

4. **Changelog entry** flagging the breaking change for users who
   had muscle memory for the old `prefix t = always-new` shape.

### Notes

- `cmd-t` / `cmd-e` remain on `codon_session::GotoOrOpen*` —
  unchanged.
- The user's `~/.config/codon/codon.toml` is not modified; users
  with explicit overrides keep their existing bindings.
- `cmd-shift-e` (today: `GotoOrOpenEditor`) is left alone. Editor
  doesn't get a `prefix shift-e` because `prefix e` is the file-
  manager chord; editor focus already lives on `cmd-shift-e`.

## Acceptance

- `prefix t` with no terminal in the session opens a new one;
  `prefix t` with an existing terminal focuses it. Same for
  `prefix e` and file-manager.
- `prefix shift-t` / `prefix shift-e` always produce a fresh pane,
  even when an instance exists.
- `spec lint` clean.

## Files touched

- `crates/codon-keymap/src/keymap.rs` — embedded TOML rewrite for
  the four chords.
- `crates/file-manager/src/file_manager.rs` (or a sibling) — new
  `OpenNew` action (or rename + reroute existing).
- `assets/config/codon.example.toml` — example mnemonic comment.
- Tests in `crates/codon-session/` covering the goto-or-open
  paths if not already present.
