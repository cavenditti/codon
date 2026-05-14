---
id: REQ:codon/fm-chrome
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  File manager window chrome is condensed to two bars — one top, one
  bottom — with contextual hint overlays replacing on-screen help and
  status duplication.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-10]
---

# File manager chrome

The file manager today renders four bars of chrome (header chips on
top, plus rich-info / status / help stacked at the bottom) which
duplicates information and crowds the listing. The chrome is
consolidated to one top bar and one bottom bar; help becomes
contextual rather than always-on.

:::{requirement id="fm-chrome" level="MAY"}
The system SHOULD provide:

- {#c-top-bar} A single top bar that shows the current directory path
  on the left and the existing header chips (sort / filter / find /
  hidden) on the right.
- {#c-bottom-bar} A single bottom bar that, by default, shows the
  focused-entry rich-info segments on the left and listing totals
  (with the `position/total` counter) on the right.
- {#c-contextual-hints} When a pending input prompt or other
  state-driven hint context applies, the bottom bar's left segments
  are replaced with the contextual key hints (the previous
  always-on help bar's content), so hints are surfaced only when
  task-relevant.
- {#c-cmd-shortcuts} While Cmd is held and no other modifier is
  pressed, the bottom bar's left segments are replaced with a
  compact general-shortcuts row so a power user can glance at the
  fixed bindings on demand. Releasing Cmd, or pressing any other
  modifier, restores the previous left content.
- {#c-precedence} Pending-input contextual hints outrank
  Cmd-shortcuts: when both apply, the contextual hint row is shown.
- {#c-no-status-bar} The standalone path / position status row is
  removed; the path lives on the top bar and the position counter
  joins the bottom-right totals.
- {#c-no-help-toggle} The user-facing help-bar toggle and the
  rich-info toggle are retired — the consolidated layout has nothing
  optional to hide.
:::
