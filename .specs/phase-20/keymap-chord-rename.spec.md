---
id: TASK:phase-20/keymap-chord-rename
type: task
status: draft
version: 0.0.1
summary: >
  Swap the window-verb sub-prefix from `prefix shift-w` to `prefix w`,
  bind `prefix shift-w` as the single-chord window overview, drop the
  bare `prefix l` (WindowLast) leaf, and remove the `prefix p X`
  picker sub-chord chain (pickers move to the `space`-leader flow in
  the sibling task). Keep `prefix n` / `prefix p` as bare-leaf
  WindowNext / WindowPrev muscle-memory chords.
owners: [carlo]
progress: done
refines:
  - REQ:codon/keymap-vocabulary#c-chord-window-prefix
  - REQ:codon/keymap-vocabulary#c-chord-window-nav-leaves
aspects: [window-family-swap, nav-leaves]
---

# Keymap chord rename — window family + picker chain

## Plan

Pure embedded-defaults rewrite plus an example-config update. No
dispatcher changes.

### Embedded defaults — `crates/codon-keymap/src/keymap.rs`

**Remove:**

```toml
# Old window family on shift-w
"prefix shift-w n" = "codon_session::WindowNew"
"prefix shift-w l" = "codon_session::WindowNext"
"prefix shift-w h" = "codon_session::WindowPrev"
"prefix shift-w shift-l" = "codon_session::WindowLast"
"prefix shift-w c" = "codon_session::WindowClose"
"prefix shift-w w" = "codon_session::WindowSwitch"
"prefix shift-w o" = "codon_session::WindowOverview"
"prefix shift-w r" = "codon_session::WindowRename"
"prefix shift-w !" = "codon_session::BreakPaneToWindow"

# Old `prefix w` close binding (phase-20 verb-collapse-close keeps
# `cmd-w` as the only short-chord close path).
"prefix w" = "codon_session::SafeCloseActiveItem"

# Old `prefix l` bare-leaf WindowLast
"prefix l" = "codon_session::WindowLast"

# Old picker sub-prefix chain
"prefix p f"       = "file_finder::Toggle"
"prefix p b"       = "tab_switcher::Toggle"
"prefix p s"       = "outline::Toggle"
"prefix p shift-s" = "project_symbols::Toggle"
"prefix p d"       = "diagnostics::Deploy"
"prefix p shift-d" = "diagnostics::Deploy"
"prefix p r"       = "projects::OpenRecent"
"prefix p g"       = "codon_pickers::ChangedFilesPicker"
"prefix p j"       = "codon_pickers::JumplistPicker"
"prefix p '"       = "codon_pickers::LastPicker"
```

**Add:**

```toml
# New window family on `prefix w`
"prefix w n"       = "codon_session::WindowNew"
"prefix w l"       = "codon_session::WindowNext"
"prefix w h"       = "codon_session::WindowPrev"
"prefix w shift-l" = "codon_session::WindowLast"
"prefix w c"       = "codon_session::WindowClose"
"prefix w w"       = "codon_session::WindowSwitch"
"prefix w r"       = "codon_session::WindowRename"
"prefix w !"       = "codon_session::BreakPaneToWindow"

# Single-chord overview (was the sub-prefix head)
"prefix shift-w"   = "codon_session::WindowOverview"
```

**Unchanged:**

```toml
# Bare-leaf muscle-memory window nav stays
"prefix n" = "codon_session::WindowNext"
"prefix p" = "codon_session::WindowPrev"
"prefix r" = "codon_session::WindowRename"   # keep generic-r alias
"prefix !" = "codon_session::BreakPaneToWindow"
```

### Example config — `assets/config/codon.example.toml`

Update the `# Tmux-parity chords that codon defaults now bind under ctrl-x:`
comment block to reflect the new shape, and remove any references
to the old `prefix p X` picker chain.

### User config note

The user's `~/.config/codon/codon.toml` is not modified. Existing
`prefix shift-w …` overrides keep working only if the user re-adds
them; the changelog flags the rebind so users with muscle memory
on the old shape are warned.

### Sequencing

This task is the chord-rename half of phase 20's vocabulary work.
The `space`-leader picker flow lands in
[TASK:phase-20/space-leader-pickers](spec:TASK:phase-20/space-leader-pickers)
— pickers must remain reachable somehow, so the two tasks ship
together (this one removes the old chord chain, the sibling adds
the new flow). The verb-collapse trio (split / open-or-focus /
close) is independent and may ship in either order.

## Acceptance

- After the rebind, `prefix w` plus a continuation key fires the
  expected window verb (test each: n / l / h / shift-l / c / w / r / !).
- `prefix shift-w` fires `WindowOverview` immediately (single
  chord, no continuation).
- `prefix l`, `prefix w` (bare), and `prefix p <letter>` chords
  are all unbound (matcher dead-end).
- `prefix n` / `prefix p` continue to fire WindowNext / WindowPrev.
- Cheatsheet (`prefix <F1>`) shows the new chord shapes; no stale
  `prefix shift-w …` entries linger.
- `spec lint` clean.

## Files touched

- `crates/codon-keymap/src/keymap.rs` — embedded TOML.
- `assets/config/codon.example.toml` — comment block + any
  example-binding references.
- (No dispatcher changes — all chords map to existing actions.)
