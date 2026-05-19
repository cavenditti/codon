---
id: TASK:phase-19/terminal-blocks-osc133
type: task
status: draft
version: 0.0.1
summary: >
  OSC 133 parser in the vendored `alacritty_terminal` event
  stream + `BlockStore` per-terminal-pane entity + `Block`
  typed object plumbed through `codon_pane_bridge::Selection`.
  Foundation task — heuristic detection, navigation, and
  cross-pane verbs ride on top in follow-up tasks.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/terminal-blocks#c-block-object
  - REQ:codon/terminal-blocks#c-osc-133-parser
  - REQ:codon/terminal-blocks#c-selection-source
aspects: [block-object, osc133-parser, selection-source]
---

# OSC 133 parser + BlockStore + Block object

## What ships

The minimum surface that makes a Block exist as a typed selection.
Follow-ups (`phase-19/terminal-blocks-heuristic`,
`phase-19/terminal-blocks-navigation`,
`phase-19/terminal-blocks-cross-pane`,
`phase-19/terminal-blocks-shell-snippets`) build on this.

1. **OSC 133 sequence parsing** in `vendor/zed/crates/terminal/`.
   The existing alacritty `EventListener` already surfaces OSC
   escape sequences; add a handler arm matching `133;A`, `133;B`,
   `133;C`, `133;D[;exit]` and emit a `BlockBoundary` event on the
   pane entity's channel.

2. **`BlockStore`** in a new module
   `crates/codon-panes/src/terminal_blocks.rs` (or similar — the
   final placement may be `codon-pane-bridge` if multiple crates
   need read access). Reassembles `BlockBoundary` events into
   `Block { command, output, exit_status, start, end, detection }`
   records keyed by terminal pane id. Out-of-order boundaries
   degrade gracefully — the partial block is dropped, scanning
   continues at the next `A`.

3. **`ObjectKind::Block` + `Selection::Blocks(Vec<TerminalBlockRef>)`**
   in `codon-pane-bridge`. `TerminalBlockRef` is
   `{ pane: WeakEntity<Terminal>, index: usize }`.

4. **Terminal pane `SelectionSource` impl** returns
   `Selection::Blocks(...)` when block-selection state is active
   on the pane, falling back to existing text selection otherwise.
   No navigation verbs yet — block-selection state is set only via
   programmatic test API in this task.

## Out of scope

- Heuristic detection (separate follow-up task).
- Block-aware Normal-mode bindings (`]b` / `[b` / `mib`).
- Cross-pane verb wiring (`codon_agent::Explain` accepting Block).
- Shell-integration installer / shell snippets.
- Status-bar indicator for detection mode.

## Verification

- Unit tests in `terminal_blocks.rs` exercise the boundary state
  machine: clean ABCD sequence → one Block; AB[gap]A → drop first,
  start fresh; ABCD without exit → exit_status = None; out-of-order
  ACBD → drop.
- Integration test: a `MockTerminal` emits a canned ABCD sequence
  with known text; `BlockStore` reports one Block with the right
  command + output + exit_status.
- Smoke: in a real codon window, source the future shell snippet
  by hand (`printf '\e]133;A\e\\'…`) and confirm the
  `BlockStore` accumulates records — visible via a temporary
  debug-log action.

## Files touched

- `vendor/zed/crates/terminal/src/`: OSC 133 dispatch arm.
- `crates/codon-pane-bridge/src/codon_pane_bridge.rs`: `Block` /
  `Selection::Blocks` variants + `TerminalBlockRef`.
- New: `crates/codon-panes/src/terminal_blocks.rs`.
- `crates/codon-panes/Cargo.toml`: deps if any new (probably none).
