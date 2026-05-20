---
id: TASK:phase-20/binding-hints-everywhere
type: task
status: draft
version: 0.0.1
summary: >
  Every UI surface that displays a verb name MUST also render the
  verb's currently-bound chord (or a "—" placeholder if unbound).
  Audit: command palette, cheatsheet, action-history picker, peek-
  dock footer, toast notifications. Add a unit-test or lint that
  catches new surfaces naming verbs without chords.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/discoverability#c-binding-hints-everywhere
---

# Binding hints everywhere

## Plan

### Surfaces to audit

1. **Command palette** — `crates/codon-command-palette/`. Today
   palette rows render the verb's display name only. Add a chord
   column rendered via `cx.bindings_for_action(action)`.

2. **Cheatsheet modal** — `crates/codon-keymap/src/cheatsheet_modal.rs`.
   Already shows chords (it IS a chord listing). Verify the new
   `prefix w …` / `space …` chords appear after
   [TASK:phase-20/keymap-chord-rename](spec:TASK:phase-20/keymap-chord-rename)
   and
   [TASK:phase-20/space-leader-pickers](spec:TASK:phase-20/space-leader-pickers).

3. **Action-history picker** — introduced by
   [TASK:phase-20/action-history-ring](spec:TASK:phase-20/action-history-ring).
   Chord column required from day one; this task ensures the
   convention.

4. **Peek-dock footer** — `crates/codon-panes/`. Today the peek
   dock renders the panel title; add a short footer listing the
   3-5 most useful chords for that panel kind (e.g. agent panel:
   `prefix a e (explain)`, `prefix a s (summarize)`,
   `cmd-w (dismiss)`).

5. **Toast notifications** — any path that emits a toast mentioning
   an action by name (search the codebase for
   `gpui::Workspace::show_notification` or similar) MUST render the
   chord alongside the verb. Likely candidates: hold-quit toast,
   pane-close cascade toasts, agent error toasts.

### Helper

A single helper in `codon-keymap`:

```rust
pub fn chord_for_action(
    cx: &App,
    action_name: &str,
    context: Option<&KeyBindingContextPredicate>,
) -> Option<String>;
```

Returns the human-readable chord string (e.g. `"ctrl-x w n"`) for
the action under the given context, or `None` if unbound.
Implementation wraps `cx.bindings_for_action(...)` and renders the
first matching binding's `KeyBinding::keystrokes()` via the existing
`ui::KeyBinding::from_keystrokes` formatter.

### Lint

Add a unit test in `crates/codon-keymap/` (or a sibling integration
test) that walks every surface listed above and asserts the chord
column is present. The walk uses a small registry of "surfaces that
name verbs" so new surfaces opt in at registration time.

A stricter compile-time lint (e.g. a proc-macro-derive that enforces
the chord-rendering convention) is out of scope — too much
infrastructure for the discoverability win.

### Rebind reflection

Because the helper uses the live GPUI registry, a user rebind takes
effect on the next render without restart. Tests should cover at
least one rebind path (load a `~/.config/codon/codon.toml` with a
custom chord, render a palette row, assert the new chord appears).

## Acceptance

- Open the command palette; every row shows a chord column.
- Open the cheatsheet; every binding listed has its chord.
- Open a peek dock; footer shows 3-5 chords with verb names.
- Trigger a toast that names an action; chord appears in the
  toast text.
- Rebind a chord in user codon.toml and reload — the palette
  row reflects the new chord without restart.
- New surface added to the registry without chord column fails
  the unit test.
- `spec lint` clean.

## Files touched

- `crates/codon-keymap/src/lib.rs` — `chord_for_action` helper.
- `crates/codon-command-palette/` — palette row layout.
- `crates/codon-keymap/src/cheatsheet_modal.rs` — verify, no
  layout change expected.
- `crates/codon-panes/` — peek-dock footer.
- Any toast emit sites — sweep + add chord rendering.
- Tests covering each surface + the lint.
