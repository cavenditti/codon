---
id: TASK:phase-20/dead-end-flash
type: task
status: draft
version: 0.0.1
summary: >
  Surface a ~200 ms status-bar colour flash when the keystroke
  matcher hits a terminal-but-empty state — unmapped keystroke or
  chord-prefix timeout with no bound continuation. No toast, no
  log line, no focus change. The signal is purely "your keystroke
  was seen and produced no bound action."
owners: [carlo]
progress: pending
refines:
  - REQ:codon/discoverability#c-dead-end-flash
---

# Dead-end colour flash

## Plan

### Detection — vendored Zed

The GPUI keymap matcher lives in
[`vendor/zed/crates/gpui/src/keymap/matcher.rs`](spec:src:vendor/zed/crates/gpui/src/keymap/matcher.rs).
Two states need to publish a "dead-end" event:

1. **Unmapped keystroke** — the matcher consumed a keystroke and
   no binding (full or partial-chord-prefix) matched.
2. **Chord timeout with no continuation** — the chord-prefix
   buffer accumulated keystrokes, the
   [`gpui::set_keystroke_chord_timeout`](spec:src:vendor/zed/crates/gpui/src/keymap/matcher.rs)
   timer expired, and the buffered prefix had no full-binding
   completion (i.e. the timeout flush produced no action).

Add a `KeystrokeOutcome` enum or a dedicated event:

```rust
pub enum KeystrokeOutcome {
    Matched,           // a binding fired
    PartialChord,      // accumulating, waiting for next keystroke
    DeadEnd,           // unmapped or timed-out with no completion
    Passthrough,       // self-insert into focused element
}
```

The matcher publishes the outcome via an existing or new GPUI
event channel — pick the lowest-touch mechanism that the codon
status bar can subscribe to without coupling to internals.

### Subscription — codon-mode

The status bar in
[`crates/codon-mode/src/mode_indicator.rs`](spec:src:crates/codon-mode/src/mode_indicator.rs)
subscribes to `KeystrokeOutcome` events. On `DeadEnd`:

- Arm a 200 ms animation that flashes the status bar background
  colour (theme-aware token; pick the existing "warning subdued"
  or sibling colour token, must be legible on both light and dark
  themes).
- During the flash, the status bar text remains rendered normally
  on top.
- Coalesce repeated dead-ends within 200 ms into a single flash
  (don't strobe on a held key).

### Out of scope

- Distinguishing "unmapped" from "chord timeout" in the UI. Both
  surface as the same flash — the user's keystroke was seen and
  did nothing; mechanism distinction is noise.
- Logging dead-ends to disk. The flash is sufficient; a developer
  debugging a binding can use `RUST_LOG=codon_keymap=debug`.
- Toasting the dead-end (explicit non-goal — too loud for a
  miss-typed chord).

### Vendored Zed sequencing

The matcher extension is the only vendored-Zed touch. The codon
side compiles independently — if the vendored change isn't ready,
the status-bar side can no-op while waiting.

## Acceptance

- Press an unmapped chord (e.g. `ctrl-x z` if `z` has no binding
  under the prefix) — status bar flashes once for ~200 ms.
- Press the chord prefix and wait for the chord timeout to fire
  with no bound completion — status bar flashes once.
- Press a successfully-bound chord — status bar does NOT flash.
- Press the prefix mid-chord and then continue to a bound chord
  — no flash (the partial-chord state is not a dead-end).
- Hold a dead-end key for 1 s — single flash (coalesced), not a
  strobe.
- Theme switch — flash colour remains legible.
- `spec lint` clean.

## Files touched

- `vendor/zed/crates/gpui/src/keymap/matcher.rs` — outcome
  enum + event emission.
- `crates/codon-mode/src/mode_indicator.rs` — subscriber +
  flash animation.
- `crates/codon-mode/src/` — possibly a new `dead_end_flash.rs`
  if the animation logic deserves isolation.
- Tests on the GPUI side covering the outcome states; codon side
  tests covering the flash arming + coalescing.
