---
id: TASK:phase-22/command-history-osc133-consumer
type: task
status: accepted
version: 0.1.0
summary: >
  Subscribe `codon-command-history` to the OSC 133 `Block` events
  emitted by codon-terminal-blocks. Each completed `Block`
  inserts a NewEntry into the store with raw command + raw output
  excerpt (same trust model as shell history), then enqueues a
  summarization job whose prompt — built at egress time — routes
  through the redaction pipeline.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-history#c-source-osc133
aspects: [event-subscription]
blocked_by:
  - TASK:phase-22/command-history-store
---

# Consume OSC 133 boundary events

## Plan

- The terminal-blocks crate (existing,
  [crates/codon-terminal-blocks](spec:src:crates/codon-terminal-blocks/src/codon_terminal_blocks.rs))
  emits `Block` events when a prompt closes (`133;D` semicolon
  exit code). Confirm — and extend if needed — the event surface
  so subscribers can read: cwd, shell program, command bytes,
  output region bytes, exit code, duration.
- Add `codon_command_history::Subscriber` that registers as a
  workspace-scoped observer on terminal-blocks. The subscriber
  receives `BlockCompleted` events.
- On each event:
  1. Skip silently when `[command_history] enabled = false`.
  2. Build a `NewEntry { ts, cwd, shell, command_text,
     output_excerpt, exit_code, duration_ms }` with the raw
     bytes from the block. The output excerpt is the last 4 KiB
     of the block's output region (configurable).
  3. Insert the row through `HistoryStore::insert`. The store
     is raw — no redaction at this point.
  4. Enqueue a summarization job for the inserted row id.
- Burst coalescing: when a new event arrives within 500 ms of
  the previous one in the same cwd, the queue collapses them
  into one `NewEntry` whose `command_text` is the joined
  commands separated by ` && `. Implementation lives in the
  subscriber, not the store.
- Redaction happens at the *summarizer's* egress point, not
  here — see sibling task `command-history-summarizer`. If the
  pipeline returns `Risky` there, the summarizer calls
  `HistoryStore::mark_skipped(id, "risky_redaction")`; the row
  itself stays raw and pasteable.

## Acceptance

- Integration test: a synthetic terminal emitting three OSC 133
  blocks produces three inserted rows containing the raw command
  bytes.
- Integration test: a block whose command bytes contain
  `AWS_SECRET_ACCESS_KEY=abc` produces a row with the raw bytes
  in `command_text`. The summarizer (separate task) will later
  flip `llm_skipped = true` for that row at its egress check;
  this task is satisfied by the raw insert.
- Coalescing test: three blocks 100 ms apart in the same cwd
  collapse into one row.
- `cargo test -p codon-command-history` passes.
