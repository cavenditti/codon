---
id: REQ:codon/jump-hints
type: requirement
status: accepted
version: 1.0.0
level: SHOULD
summary: >
  Window-wide hint mode. Two entry actions render two-letter
  overlay labels above every visible word, URL, and clickable
  UI element across all panes; pressing two keys dispatches the
  candidate's action (move cursor / click / copy URL / etc.).
owners: [carlo]
categorized_under: [TOPIC:topics/phase-10]
---

# Jump hints — global Vimium-style navigation

A modal overlay activated by `cmd-k j` (any target) or
`cmd-k u` (URLs only). While active, the overlay paints
absolute-positioned 2-character chips at the top-left of every
visible candidate's bounds. Pressing two alphabet keys fires the
candidate's action and dismisses the overlay; Esc / focus change
/ scroll / non-alphabet key dismisses without firing.

The implementation reuses three pieces of vendored Zed
infrastructure already in tree:

- `gpui::Window::defer_draw` for window-level absolute layers
  (same primitive that powers tooltips).
- `editor::find_url` (`crates/editor/src/hover_links.rs`) for
  URL detection in editor buffers — wraps `linkify` with the
  full set of Zed's link-kind rules.
- `terminal::terminal_hyperlinks::URL_REGEX` + `RegexSearch`
  for URL detection over the alacritty grid.

The text-word scan inside the editor reuses the visible-range +
two-grapheme-word logic from `vim::helix::helix_jump_to_word`
(already faithful to Helix's `gw`).

:::{requirement id="jump-hints" level="SHOULD"}
The system MUST:

- {#c-overlay-core} provide a window-level `JumpOverlay` modal
  element rendered via `Window::defer_draw` above all panes,
  consuming all keystrokes while active. Painting absolute-
  positioned 2-char chips at each candidate's screen-space
  top-left. Dismiss on Esc, focus change, viewport scroll, or
  any non-alphabet keystroke; on dismiss, no action fires.
- {#c-provider-trait} define `JumpProvider` trait — a pane
  registers an entity that, on demand, yields
  `Vec<JumpCandidate>` covering its visible content
  (`JumpKind::Word`, `JumpKind::Url`) plus an action closure
  per candidate. Providers register at panel-init time into a
  global `JumpRegistry` and are GC'd when the entity drops.
- {#c-clickable-wrapper} provide a `JumpClickable` element
  wrapper (in codon-jump, sibling to `ui::Button`) that any
  UI element opts into via `.jump_target(label, on_click)`.
  The wrapper registers `(bounds, on_click)` into a paint-time
  thread-local that the overlay drains on activation. Clears
  on next paint of the underlying element.
- {#c-label-alphabet} assign 2-character labels from a
  configurable alphabet (default `a..z`, 26² = 676 labels).
  Greedy nearest-to-cursor: the candidate closest to the
  primary cursor / mouse / focus gets the lexically-first
  available label. When candidate count > alphabet², degrade
  to 3-char labels (cap at alphabet³). When count > alphabet³,
  cap and silently drop the furthest candidates.
- {#c-keystroke-loop} two-keystroke capture with progressive
  narrowing — after the first key, repaint with only labels
  whose first character matched, second char highlighted. If
  exactly one candidate matches after the first key, auto-fire
  without waiting for the second. Esc or any non-alphabet key
  cancels at any depth.
- {#c-jump-targets} `codon_jump::JumpToTarget` (default chord
  `cmd-k j`) activates jump mode covering Word + Url +
  Clickable candidates from every registered provider. Each
  candidate's action runs as-declared (move cursor / focus +
  select / click / etc.).
- {#c-jump-urls} `codon_jump::JumpToUrl` (default chord
  `cmd-k u`) filters the candidate set to `JumpKind::Url`
  only; the selected URL is copied to the system clipboard
  via `cx.write_to_clipboard(...)` and a `MessageNotification`
  toast confirms ("Copied <url>").
- {#c-pane-editor} the editor crate registers a provider
  yielding (a) visible-region words via the existing
  helix-jump scan, (b) URLs via `find_url` over the visible
  buffer range. Word action sets the primary cursor; URL
  action passes the url to the active jump-mode dispatcher.
- {#c-pane-terminal} the terminal_view crate registers a
  provider yielding (a) visible-grid words (whitespace-
  separated tokens of ≥2 chars), (b) URLs via the existing
  `URL_REGEX` `RegexSearch`. Word action focuses the terminal
  + alacritty-selects the word; URL action passes through.
- {#c-pane-file-manager} the file-manager crate registers a
  provider yielding one candidate per visible row.
  Action = set cursor index. URL candidates are skipped (fm
  doesn't render URLs).
- {#c-clickable-adoption} adopt `JumpClickable` at the
  user-visible interactive surfaces — workspace tabs, dock
  toggles, status bar items, panel headers (git, agent,
  project), notifications. Each opt-in is a 1-line
  `.jump_target(...)` addition; underlying click handlers
  unchanged.
- {#c-config-toml} `~/.config/codon/jump.toml` overrides:
  alphabet (`alphabet = "asdfghjkl;"`), label position (top-
  left / center), max candidates per provider, dismiss-on-
  scroll toggle, the two chords. Loaded once at startup with
  `Fs::watch` hot reload (same path the openers/theme configs
  use).
:::
