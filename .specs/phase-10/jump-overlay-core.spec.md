---
id: TASK:phase-10/jump-overlay-core
type: task
status: accepted
version: 0.0.1
summary: >
  New `codon-jump` crate hosting the `JumpOverlay` window-level
  modal, `JumpRegistry` global, `JumpProvider` trait, label
  alphabet + assignment, and the two-keystroke capture loop.
owners: [carlo]
progress: done
refines:
  - REQ:codon/jump-hints#c-overlay-core
  - REQ:codon/jump-hints#c-provider-trait
  - REQ:codon/jump-hints#c-label-alphabet
  - REQ:codon/jump-hints#c-keystroke-loop
aspects: [overlay-element, registry-global, provider-trait, label-assigner, keystroke-loop]
---

# Jump overlay core

## What ships

New crate `crates/codon-jump/` (single file, no sub-modules:
`src/codon_jump.rs` per the project's `mod.rs`-free convention).

Public surface:

```rust
pub trait JumpProvider {
    fn collect(&self, ctx: &JumpContext, cx: &mut App) -> Vec<JumpCandidate>;
}

pub enum JumpKind { Word, Url(String), Clickable }

pub struct JumpCandidate {
    pub bounds: Bounds<Pixels>,   // screen-space, post-scroll
    pub kind: JumpKind,
    pub action: Box<dyn FnOnce(&mut Window, &mut App) + Send>,
}

pub struct JumpContext {
    pub mode: JumpMode,            // Target | Url
    pub cursor_anchor: Option<Point<Pixels>>,  // for nearest-first
}

pub enum JumpMode { Target, Url }

pub struct JumpRegistry { /* WeakEntity-keyed provider list */ }

pub struct JumpOverlay { /* modal + state machine */ }
```

`JumpRegistry` is a `gpui::Global` holding `Vec<WeakEntity<dyn
JumpProvider>>`. Entries with dropped weak refs are skipped at
collect time and pruned periodically.

`JumpOverlay` is constructed via `JumpOverlay::open(mode, window,
cx)` — pushes itself as a `Workspace` modal layer painted via
`Window::defer_draw` so it sits above every pane. On open:

1. Build `JumpContext { mode, cursor_anchor }` from the focused
   pane's primary cursor / selection origin.
2. `cx.global::<JumpRegistry>().collect_all(ctx, cx)` to gather
   candidates.
3. Filter by mode (Url filters to `JumpKind::Url(_)`).
4. Sort candidates by Euclidean distance to `cursor_anchor` —
   closest first.
5. Run `assign_labels(alphabet, candidates)` — greedy 2-char,
   fall back to 3-char when count > alphabet², cap at alphabet³.
6. Subscribe to keystroke / focus-change / scroll events on the
   workspace; first-key narrows, second-key fires.

`assign_labels` is a pure function exposed for unit testing.
~100 LOC. Tests cover: empty, single, exact-size, overflow into
3-char, exact alphabet² boundary.

Dismissal triggers (all fire `cx.emit(DismissEvent)` without
running any candidate action):

- Esc.
- Any key not in the alphabet (after the second-char window).
- Focus changing to a different workspace pane.
- Workspace scroll event reaching the overlay (subscribed via
  `cx.subscribe(workspace, ScrollEvent)`-style hook).

Rendering: a `div().absolute().left(b.origin.x).top(b.origin.y)`
chip per candidate. Background = `cx.theme().colors().conflict`
(yellow-ish, high contrast). Foreground = `Color::Default`.
After the first key, dim non-matching chips to 30% alpha and
bold the second char of matching chips.

## Out of scope

- No providers in this task. Providers and `JumpClickable` are
  separate tasks; this task is the bare overlay + registry.
- No actions wired (`JumpToTarget` / `JumpToUrl` are
  follow-ups).
- No config TOML (default alphabet `"abcdefghijklmnopqrstuvwxyz"`
  hard-coded for now; config task swaps to TOML-driven).

## Verification

- `cargo test -p codon-jump` exercises `assign_labels` exhaustively.
- Integration test: a `MockProvider` yielding N fixed-bounds
  candidates → overlay opens, two keystrokes resolve to the
  expected candidate, action closure fires.
- Smoke: `JumpOverlay::open` from a dev-only action, no providers
  registered → opens overlay showing "No targets visible", dismisses
  on Esc.

## Where it slots in

- New crate: `crates/codon-jump/`
  - `Cargo.toml` — deps: `gpui`, `ui`, `workspace`, `theme`, `anyhow`, `log`.
  - `src/codon_jump.rs` (library root via `[lib] path = "..."`).
- Workspace `Cargo.toml` — add to `members`.
- Workspace deps section — `codon-jump.workspace = true`.
