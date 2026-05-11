---
id: REQ:codon/command-palette
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Keyboard-first command palette opened with `:` in Normal mode. Shows
  a live description for the active row, supports typed argument
  completers (`:e <path>`, `:theme <name>`), and is fully driveable
  without the mouse.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-5]
---

# Command palette

## Context

Zed's `command_palette` is the right substrate — it already enumerates
every registered `Action`, fuzzy-matches by humanized name, and shows
the keystroke if one is bound. Two gaps make it feel mouse-centric in
practice:

1. **Action descriptions live on hover.** Each command's doc comment
   is rendered through Zed's tooltip system, which a keyboard user
   never sees. There's no always-visible description pane next to the
   active row.
2. **Commands take no arguments.** `:open <file>`, `:theme <name>`,
   `:rg <pattern>`, `:line <n>` — every "verb + argument" flow that
   Helix users expect — has no representation. You launch the command,
   then a *separate* picker opens (or worse, an OS file dialog).

Codon's modal layer also lets us re-anchor entry: `:` in Normal mode
should open the palette, mirroring Helix and vim — `cmd-shift-p` stays
as the platform-native alternative.

## Goals (and non-goals)

- **Goal.** Hit `:`, type a few letters, *see what the command does*,
  type `<space>` to enter the argument (e.g. file path) with
  type-to-filter completion, hit `enter` to run. Everything reachable
  via keystroke.
- **Non-goal (phase 1).** Reimplementing Zed's command registry. The
  underlying `CommandPaletteFilter` + `Action` registration stay
  unchanged; we wrap the picker delegate, not the dispatch path.
- **Non-goal (phase 1).** Typed argument schemas for every command.
  We hand-pick a small whitelist of high-traffic verbs (`:open`/`:e`,
  `:theme`, `:line`, `:rg`/`:search`); the rest fall through to the
  existing argument-less behaviour.

## Approach — two layers (A → B)

**Layer A — wrapper crate.** A new `crates/codon-command-palette`
wraps Zed's `CommandPaletteDelegate`: it owns the modal, the binding
on `:`, and the description-pane render. The palette body is the
existing Zed picker — we don't fork it. A small completer registry,
keyed by command name (or alias), turns "user typed `:e ` (note the
space)" into a sub-picker fed by a `Completer` impl. The sub-picker
is its own `Picker<...>` over the completer's items; `Enter` builds
and dispatches the original action with the chosen value via the
existing Zed dispatch path. Ship value: keyboard parity with Helix
for the verbs people actually use.

**Layer B — typed argument schemas.** Later, extend the `Action`
proc-macro (or add a sibling registration call) so commands can
declare an argument schema (`File`, `Theme`, `LineNumber`, `Free`).
The palette parses `:e foo<TAB>` inline (no sub-picker push), driven
by the schema. This is upstream-Zed territory and a much bigger
surface; deferred to a later phase.

This REQ scopes Layer A. Layer B will be its own REQ once Layer A
ships and the completer interface stabilises.

:::{requirement id="command-palette" level="MUST"}
The system MUST provide:

- {#c-colon-trigger} `:` in codon Normal mode (terminal, file
  manager, editor) opens the command palette; `cmd-shift-p` continues
  to work as the platform-native equivalent
- {#c-description-pane} the palette renders an always-visible
  description block next to (or below) the active row, sourced from
  the command's doc comment via `humanize_action_name` +
  `Action::action_documentation` — never relying on a tooltip
- {#c-completer-trait} a `Completer` trait + registry in
  `codon-command-palette` keyed by action name, returning a list of
  `(value, label)` pairs filtered against the user's argument query
- {#c-builtin-completers} built-in completers for the high-traffic
  verbs: file paths (`workspace::Open`, `editor::OpenFile`), theme
  names (`theme_selector::Toggle`), line numbers (`editor::GoToLine`),
  search patterns (`workspace::NewSearch`, free-text passthrough)
- {#c-arg-subpicker} once the user types `<space>` after a command
  with a registered completer, the palette transitions into argument
  mode: query becomes the completer filter, the original action is
  remembered, `Enter` builds + dispatches the action with the chosen
  value, `Esc` returns to command mode
- {#c-keyboard-parity} every interaction in the palette — cycle rows,
  inspect description, jump to argument mode, run, dismiss — is bound
  to a keystroke; nothing is mouse-only
- {#c-fallback} commands without a registered completer keep the
  current Zed behaviour: hitting `Enter` dispatches the action
  immediately (the action itself may open a follow-on picker, e.g.
  file finder)
:::

## Reference points

- [`vendor/zed/crates/command_palette/src/command_palette.rs`](spec:src:vendor/zed/crates/command_palette/src/command_palette.rs)
  — `CommandPaletteDelegate`, `humanize_action_name`, the existing
  picker structure.
- [`vendor/zed/crates/picker/src/picker.rs`](spec:src:vendor/zed/crates/picker/src/picker.rs)
  — `Picker<D>` + `PickerDelegate` we reuse for the argument
  sub-picker.
- [`crates/codon-keymap/src/cheatsheet_modal.rs`](spec:src:crates/codon-keymap/src/cheatsheet_modal.rs)
  — pattern for a codon-owned full-screen `ModalView`.
- [`vendor/zed/crates/file_finder/src/file_finder.rs`](spec:src:vendor/zed/crates/file_finder/src/file_finder.rs)
  — file-path completer reference (PathMatcher + fuzzy::match_strings).

## Open questions (for Layer B, not this REQ)

- Does typed-argument support belong on Zed's `Action` derive, or in a
  sibling `actions_with_args!` macro? Upstream Zed has no concept of
  argumented actions today.
- How do completers compose with the existing `CommandInterceptResult`
  flow (used for `:line` already, sort of)?

Effort estimate: Layer A is **medium-large** (~500-700 LOC across
the new crate, the description pane, and 4 built-in completers).
