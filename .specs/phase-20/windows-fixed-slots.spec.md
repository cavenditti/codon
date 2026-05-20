---
id: TASK:phase-20/windows-fixed-slots
type: task
status: accepted
version: 0.0.1
summary: >
  Pre-materialise WINDOW_SLOTS (9) windows per session at creation /
  load, address every slot by stable index, and hide empty slots from
  every UI surface that lists windows (indicator, picker, overview).
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-fixed-slots
  - REQ:codon/windows#c-emptiness-rule
aspects: [fixed-slots-data-model, emptiness-filter]
categorized_under: [TOPIC:topics/phase-20]
---

# Always-on window slots

## What ships

- `crates/codon-session/src/session.rs` exports
  `pub const WINDOW_SLOTS: usize = 9`. `Session::new` constructs all
  nine windows up front with default names `"1"…"9"`.
- `Session::pad_to_window_slots()` grows legacy persisted sessions on
  registry load (`crates/codon-session/src/registry.rs::load_from`).
  Existing `WindowId`s are preserved; new slots get fresh ids derived
  from `next_window_id`.
- `Window::has_user_content(idx)` is the canonical "in use" predicate:
  `layout_has_items() || name != Window::default_name_for(idx)`.
  Renaming an empty slot deliberately marks it as in-use so the user
  can pre-claim a slot before populating it.
- `Session::displayed_window_indices()` returns the indices that show
  in UI surfaces. Always includes the active slot so the user can see
  where they are even when the active slot is itself empty.
- `Session::first_empty_window_index()` finds the lowest unused slot;
  drives `WindowNew` and `BreakPaneToWindow`.
- `Session::clear_window(idx)` resets a slot back to its untouched
  state — drops `layout`, clears `layout_stale`, restores the default
  name. The `WindowId` is preserved so cache lookups stay valid.
- Indicator (`window_indicator.rs`), picker (`window_picker.rs`), and
  overview (`overview.rs::build_rows`) all iterate
  `displayed_window_indices()` instead of `0..session.windows.len()`.
- Actions:
  - `WindowNew` hops to `first_empty_window_index`, or shows a toast
    when every slot is in use.
  - `WindowGoto(N)` keeps its existing bounds-check; empty targets
    materialise via the existing `replace_center_with_empty_pane`
    fallback in `switch_to_window`.
  - `WindowClose` clears the active slot (with the existing dirty-
    item save prompt) and jumps to the last-active populated slot
    or slot 0. Already-empty active slots short-circuit before the
    prompt.
  - `BreakPaneToWindow` plants the broken pane in
    `first_empty_window_index`, with a toast on saturation.
  - `MovePaneToWindow(N)` uses the static bounds-check and
    `clear_window` for the single-pane source case.
  - `WindowNext` / `WindowPrev` cycle through
    `displayed_window_indices` only.
- Helpers `LayoutSnapshot::has_any_items` and
  `Member::has_any_items` ride along in `vendor/zed` so the
  emptiness predicate is one call away wherever a pane tree is in
  scope.

## Tests

`session::tests` covers slot pre-materialisation, pad-on-load,
`first_empty_window_index` (fresh / partial / rename / saturated),
`displayed_window_indices`, and `clear_window`.

## Why this shape

Codon's mental model is "windows always exist, only visible when
non-empty". The dense-vec encoding lets every existing call site
(picker, overview, runtime cache key, `previous_window`) keep its
shape; the visibility carve-out lives entirely in the filter
applied at the rendering boundary. Renaming an empty slot is a
legitimate "claim this slot for later" signal, so the in-use rule
includes the name check rather than checking layout alone.
