---
id: TASK:phase-10/jump-clickable-wrapper
type: task
status: accepted
version: 0.0.1
summary: >
  `JumpClickable` element wrapper — any UI element opts in via
  `.jump_target(on_click)` and gets registered as a paint-time
  candidate the overlay can hint.
owners: [carlo]
progress: done
refines:
  - REQ:codon/jump-hints#c-clickable-wrapper
aspects: [wrapper-element, paint-time-registry]
---

# JumpClickable element wrapper

## What ships

An `IntoElement`-implementing wrapper in `codon-jump`:

```rust
pub trait JumpClickableExt {
    fn jump_target(
        self,
        on_click: impl Fn(&mut Window, &mut App) + Send + Sync + 'static,
    ) -> JumpClickable<Self> where Self: Element + Sized;
}

pub struct JumpClickable<E> { inner: E, on_click: Arc<...> }

impl<E: Element> Element for JumpClickable<E> {
    fn paint(&mut self, ..., window, cx) {
        // Get bounds from the inner element's paint pass.
        let bounds = self.inner.layout_bounds();
        // Register into the paint-time thread-local.
        ClickableRegistry::push(bounds, self.on_click.clone());
        self.inner.paint(...);
    }
}
```

`ClickableRegistry` is a `RefCell<Vec<...>>` inside a
`Window`-bound `WindowGlobal`. It's *cleared by the JumpOverlay
at open time*, not on every paint — so the first time a candidate
is collected after open, the registry holds whatever was painted
on the previous full frame. The overlay calls
`window.refresh()` after opening to force a fresh paint pass,
then drains the registry.

This avoids the chicken-and-egg "paint to discover, but the
overlay itself needs to paint" cycle: by the time the overlay's
own paint runs, every clickable has already pushed its entry.

Drop on focus change: when the overlay dismisses, the registry
stays. When the workspace repaints (e.g., a tab focus shift),
all clickables push fresh entries. Stale entries from the prior
frame are harmless because the overlay only drains on open.

## Verification

- `cargo test -p codon-jump`:
  - A no-op test element wrapping a `Button`, painted into a
    `TestVisualContext`, registers its bounds.
  - Two wrapped buttons in different positions register both.
  - Clicking a registered candidate's action via the stored
    closure invokes the original `on_click`.

## Where it slots in

- Edit: `crates/codon-jump/src/codon_jump.rs` — append the
  wrapper + `ClickableRegistry` window-global.
- No vendor/zed changes — the wrapper composes over any
  `Element` so existing buttons opt in by chaining.

## Out of scope

- Adoption at call sites (`workspace::tabs`, status bar, etc.)
  is the separate `jump-clickable-adoption` task.
- The overlay's own draining of the registry is handled in
  `jump-overlay-core`; this task is only the producer side.
