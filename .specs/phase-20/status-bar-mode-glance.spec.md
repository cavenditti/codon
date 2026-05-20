---
id: TASK:phase-20/status-bar-mode-glance
type: task
status: draft
version: 0.0.1
summary: >
  On every pane focus change or pane-mode transition, briefly render
  the 3-5 highest-frequency verbs available in the new mode at the
  right edge of the status bar. The glance decays after ~2 s or the
  next non-motion keypress. Verb set per pane × mode is curated in a
  new `[glance]` TOML table — not derived from a usage histogram, so
  new-user behaviour stays predictable.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/discoverability#c-status-bar-mode-glance
---

# Status-bar mode glance

## Plan

### Where it renders

The codon status bar lives in
[`crates/codon-mode/src/mode_indicator.rs`](spec:src:crates/codon-mode/src/mode_indicator.rs).
Add a new render slot to the right of the mode indicator (or to the
left, behind the mode indicator if the right edge is reserved for
cursor position — pick at implementation time, document choice).

### When it fires

Subscribe to the existing `CodonModeTracker` change notifier (in
`codon-pane-bridge`). On every transition where either the focused
pane kind or the pane mode changes, set the glance state to the
new (pane, mode) pair and arm a 2-second decay timer.

Also dismiss the glance early on the next non-motion keypress —
this requires either an action-fired hook (the same one used by
[TASK:phase-20/action-history-ring](spec:TASK:phase-20/action-history-ring))
or a keystroke-fired hook. Reuse whichever the ring task lands on.

### Curated verb table

New top-level `[glance]` table in
`crates/codon-keymap/src/keymap.rs`'s embedded defaults (and
parseable from `~/.config/codon/codon.toml`):

```toml
[glance.editor.normal]
verbs = ["d (delete)", "c (change)", "y (yank)", "s (select)", "?"]

[glance.terminal.normal]
verbs = ["w (next block)", "b (prev block)", "y (copy)", ":"]

[glance.file_manager.normal]
verbs = ["j/k (move)", "enter (open)", "y (yank path)", ", h (hidden)", ":"]

[glance.git_panel.normal]
verbs = ["j/k (move)", "s (stage)", "u (unstage)", "i (msg)", ":"]
```

The table is a curated start. Five verbs per row is the upper bound;
fewer is fine. The label format is `<chord> (<short verb name>)` —
chord on the left so it's scannable.

The curated default + `[glance]` overrides in user codon.toml lets
power users tune their own per-pane prompt without us having to
guess a usage histogram. Empty `verbs = []` MUST hide the glance for
that pane × mode (escape hatch for users who find it noisy).

### Style

- ~2-second linear fade-out.
- Theme-aware foreground colour (use the existing status-bar
  secondary text colour or a sibling token).
- No background highlight, no border — purely text. Must not
  visually compete with the mode indicator.
- Cancel on next non-motion keypress.

### Out of scope

- Per-user usage-histogram ranking. Curated table only for v1.
- An expanded "show all verbs" hover/keyboard surface — that's the
  cheatsheet's job
  ([TASK:phase-20/cheatsheet-pane-context](spec:TASK:phase-20/cheatsheet-pane-context)).
- Glance for Insert mode by default. Add `[glance.<pane>.insert]`
  if user feedback wants it.

## Acceptance

- Switching focus from a terminal pane to a file-manager pane
  shows the FM verb glance for ~2 s.
- Pressing any motion (h/j/k/l, arrow keys, pane-focus chord)
  does NOT dismiss the glance.
- Pressing any non-motion action (e.g. `enter`, `:`, `s`)
  dismisses the glance immediately.
- Setting `[glance.editor.normal] verbs = []` in user config
  hides the glance for that pane × mode after a keymap reload.
- `spec lint` clean.

## Files touched

- `crates/codon-mode/src/mode_indicator.rs` — new render slot.
- `crates/codon-keymap/src/keymap.rs` — `[glance]` TOML schema +
  embedded defaults.
- `crates/codon-pane-bridge/src/` — mode-tracker subscriber if
  not already exposed.
- Tests covering the curated-table parse and decay timing.
