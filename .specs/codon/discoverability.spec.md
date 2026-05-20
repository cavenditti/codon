---
id: REQ:codon/discoverability
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Five runtime affordances that teach the codon keymap at the moment
  of use, so the user can perform most actions without memorising
  chords: action-history repeat (`.` plus a `prefix ;` ring picker),
  status-bar mode glance on every mode transition, bound-chord
  rendering wherever a verb appears in UI, a 200 ms colour flash on
  chord dead-end / timeout, and a pane-aware cheatsheet that opens
  with the focused pane pre-selected.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-20]
---

# Codon discoverability

## Context

Codon's chord vocabulary, even after the phase-20 vocabulary cleanup
([REQ:codon/keymap-vocabulary](spec:REQ:codon/keymap-vocabulary)),
contains more verbs than any one user wants to memorise. Three teaching
channels exist today:

- The which-key overlay
  ([`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs))
  fires after the chord prefix and lists continuations — solves the
  "what comes after `prefix`" question.
- The command palette
  ([`crates/codon-command-palette/`](spec:src:crates/codon-command-palette))
  lets the user fuzzy-search any verb without knowing its chord.
- The full cheatsheet (`prefix <F1>`) lists every binding by pane
  and mode.

The gap is **everything between those three**: a chord that ends in
silence (typo, half-remembered prefix), a verb that the user knows
exists but can't recall the chord for, a pane the user just switched
into whose Normal mode has different verbs than the previous one, and
the long tail of actions that aren't worth memorising but are worth
re-firing if you just used one. Phase 20 adds five small affordances
that close each gap:

- **Action-history repeat.** `.` re-fires the last non-motion action;
  `prefix ;` opens a ring picker of the last ~10. The palette becomes
  a teaching surface — invoke a verb once, repeat it with one key.
- **Status-bar mode glance.** Every mode transition flashes the 3–5
  highest-frequency verbs for the new mode at the right edge of the
  status bar, decaying after ~2 s or the next keypress. No modal, no
  click.
- **Bound-chord rendering everywhere.** Anywhere a verb name appears
  in UI (palette rows, cheatsheet, peek-dock footer, context menus,
  toast notifications) the current chord renders beside it. The
  rendering is sourced from the live binding registry so a user
  rebind reflects without a restart.
- **Dead-end colour flash.** A chord that dead-ends (unmapped, or
  times out mid-chord with no continuation) flashes the status bar
  for ~200 ms. No text, no toast — just a passive signal that "the
  keystroke registered, nothing was bound." Lets the user instantly
  distinguish a missed key from a wrong key.
- **Pane-aware cheatsheet.** `prefix <F1>` today opens with a
  fixed tab order. The new shape opens with the focused pane's
  tab pre-selected, with sections in the order *global → focused
  pane → other panes*. Answers "what can I do here" before
  "what can I do anywhere."

:::{requirement id="discoverability" level="MUST"}
The system MUST provide:

- {#c-action-history-ring} a global action-history mechanism that
  tracks the last N (default 10, configurable) non-motion actions
  fired by the user across any pane, with `.` (Normal-mode in every
  pane kind) bound to re-fire the most recent entry and `prefix ;`
  bound to open a picker over the ring. Each codon-side action MUST
  declare a `is_motion: bool` (default `false`) or equivalent; only
  non-motion actions enter the ring. Repeat MUST re-fire the action
  in the *currently focused pane* (not the pane the action originally
  fired in) — this is the lever that makes the verb effectively
  context-aware. Actions whose payload depends on a transient
  selection MUST capture the payload at fire time so repeat is
  deterministic. The ring MUST persist across keymap reloads but
  MAY be in-memory only across process restarts.

- {#c-status-bar-mode-glance} a status-bar surface that, on every
  pane focus change or pane-mode transition, briefly renders the
  3–5 highest-frequency verbs available in the new mode at the
  right edge of the status bar. The glance MUST decay either after
  ~2 s or after the next non-motion keypress, whichever comes
  first. The verb set per pane × mode MUST be sourced from a
  curated table (in TOML, alongside the keymap) — not from a usage
  histogram, which would mask the discoverability win for new
  users. The glance MUST NOT consume keyboard focus and MUST NOT
  delay other status-bar updates (mode indicator, cursor position,
  etc.).

- {#c-binding-hints-everywhere} every UI surface that displays a
  verb name MUST also render the verb's currently-bound chord (or
  a "—" placeholder if unbound). At minimum:
  the command palette
  ([`crates/codon-command-palette/`](spec:src:crates/codon-command-palette)),
  the cheatsheet modal
  ([`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)),
  the action-history ring picker
  (introduced in `c-action-history-ring`), the peek-dock footer
  ([`crates/codon-panes/`](spec:src:crates/codon-panes)),
  and any toast notification that mentions an action by name.
  The chord MUST be sourced from the live GPUI binding registry
  with the current pane's predicate, so a user rebind reflects
  immediately. A lint or unit test MUST catch new UI surfaces
  that name verbs without chords (introduced as part of this
  task).

- {#c-dead-end-flash} a chord that dead-ends — unmapped keystroke,
  or chord-prefix timeout with no bound continuation — MUST trigger
  a ~200 ms colour flash on the status bar. The flash MUST NOT
  emit a toast, MUST NOT log at warn level, and MUST NOT consume
  keyboard focus. The flash colour MUST be theme-aware (legible
  on both light and dark themes). A successful chord MUST NOT
  trigger the flash. The signal is purely "your keystroke was
  seen and produced no bound action."

- {#c-cheatsheet-pane-context} the `codon_keymap::ShowKeymap`
  cheatsheet modal MUST open with the currently focused pane's tab
  pre-selected (replacing today's fixed-default tab) and MUST
  render its sections in the order *global → focused pane →
  other panes alphabetised*. The "global" section MUST stay
  collapsible (it dominates the listing). Re-invoking the
  cheatsheet from a different pane MUST re-select the new pane's
  tab.
:::

## Approach

The action-history ring sits in a new `codon-history` crate (or as
a submodule of `codon-keymap` if the surface stays small): a
`Mutex<VecDeque<HistoryEntry>>` plus a global `gpui::Global` register
read by the `codon_keymap::RepeatLast` and `codon_keymap::HistoryPicker`
actions. The "non-motion" classification is a const-time predicate per
action; codon-side actions opt in by adding `is_motion: true` to their
`actions!` definition (or a sibling macro), and a wrapper in the
dispatch path filters before push. The repeat-in-current-pane
contract is the subtle bit: each entry stores the action name +
serialised payload, not a captured handler closure, so the dispatcher
re-routes through the global action registry against the new pane's
predicate stack.

The status-bar mode-glance lives in the existing
[`crates/codon-mode/`](spec:src:crates/codon-mode/) surface as a
new render slot. The curated verbs-per-mode table joins the keymap
TOML schema as `[glance.<pane>.<mode>]` entries; the renderer reads
the table at startup and on keymap reload.

Binding-hints-everywhere is a sweep + a lint. Each UI surface that
names a verb gains a `chord_for(action_name, context)` lookup against
the live GPUI binding registry (`cx.bindings_for_action(...)`). The
lint is a unit test that walks the UI surfaces' verb-rendering call
sites and asserts the chord column is populated; new surfaces are
caught at PR time.

The dead-end flash is a hook in the GPUI matcher
([`vendor/zed/crates/gpui/src/keymap/matcher.rs`](spec:src:vendor/zed/crates/gpui/src/keymap/matcher.rs))
that publishes a "no match" event when the matcher state reaches a
terminal-but-empty state. The status-bar listens for that event and
animates the flash. Touching vendored Zed; spec the surface before
the code.

The pane-aware cheatsheet is a small edit to the modal's `open()`
path — read the focused pane's kind from the global
`CodonModeTracker` and use it as the initial-tab argument, plus a
reorder of the section iteration order.

## Out of scope

- A first-run tour / onboarding walkthrough.
- A "what's bound to this?" reverse-lookup picker — superseded by
  `c-binding-hints-everywhere` for the discovery case.
- Voice / natural-language command input.
- Usage-histogram-driven verb ranking — `c-status-bar-mode-glance`
  intentionally uses a curated table to keep new-user behaviour
  predictable.
