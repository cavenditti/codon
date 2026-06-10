---
id: TASK:phase-22/harness-cost-bookkeeping
type: task
status: accepted
version: 0.1.0
summary: >
  Record per-turn token counts when the model client returns them,
  and gate an opt-in status-bar token counter behind
  `[agent_harness] show_token_counter = true`. Default off — codon
  stays terminal-quiet.
owners: [carlo]
progress: done
refines:
  - REQ:codon/agent-harness#c-cost-bookkeeping
---

# Harness cost bookkeeping

## Plan

- Extend `PhaseEvent::ModelCallFinished` to carry
  `tokens_in: Option<u32>` and `tokens_out: Option<u32>`. Populated
  when the model client returns them; `None` otherwise.
- Per-session accumulator on the harness: `total_tokens_in`,
  `total_tokens_out` (saturating-add). Read via
  `Harness::token_totals(&self) -> (u32, u32)`.
- Status-bar item: new `codon_agent::TokenStatusItem` in
  [crates/codon-session/src/status_bar.rs](spec:src:crates/codon-session/src/status_bar.rs)
  surroundings (or wherever the status bar items currently live —
  per the phase-13 status-bar specs). Renders
  `↓ <in> ↑ <out>` with the running totals; tooltip shows per-turn
  averages.
- Config gate: `[agent_harness] show_token_counter = false` by
  default. The status-bar item is registered only when the config
  flag is true. No restart required — the config watcher rebinds
  on edit.

## Acceptance

- A turn whose stub model client returns
  `(tokens_in: 200, tokens_out: 80)` increments the harness
  totals.
- With `show_token_counter = true` set, the status bar shows
  `↓ 200 ↑ 80` after one turn.
- With the default config, the status-bar item is absent.
- `cargo test -p codon-agent` passes.
