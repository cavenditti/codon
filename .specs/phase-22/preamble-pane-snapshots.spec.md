---
id: TASK:phase-22/preamble-pane-snapshots
type: task
status: accepted
version: 0.1.0
summary: >
  Add the `PaneSnapshot` trait and implement it for every published
  pane kind. Each impl returns a ≤256-byte kind-specific summary
  the assembler concatenates into the preamble.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-context-preamble#c-pane-snapshot-trait
---

# Per-pane PaneSnapshot impls

## Plan

- Add `PaneSnapshot` trait to `crates/codon-agent/src/preamble/
  snapshot.rs`:
  ```rust
  pub trait PaneSnapshot {
      fn snapshot(&self, cx: &App) -> String;
  }
  ```
- Snapshot registry mirrors `PaneInspectRegistry`'s shape: lookup
  by `PaneKind`, returns `Option<&dyn Fn(&App) -> String>`. New
  kinds opt in via a registration call from their `init(cx)`.
- Implementations (target ≤ 256 bytes each):
  - **Terminal:** `cwd: <path>`, `shell: <prog>`, `last_exit:
    <code>` (from
    [codon-terminal-blocks](spec:src:crates/codon-terminal-blocks/src/codon_terminal_blocks.rs)
    OSC-133 detector), `prompt: <one-line trimmed prompt>`.
  - **Editor:** `file: <rel-path>`, `language: <lang>`, `cursor:
    <line>:<col>`, `dirty: <bool>`.
  - **FileManager:** `cwd: <path>`, `entries: <n>`, `marked:
    <m>`, `cursor: <name>`.
  - **Agent:** `turns: <n>`, `last_user_msg_chars: <n>`.
  - **Outline:** `symbol: <focused symbol path>`.
  - **Git:** `branch: <name>`, `dirty_files: <n>`, `staged:
    <n>`.
  - **Debug:** `state: <running/stopped>`, `frame: <fn>:<line>`.
  - **Peek:** `peeking: <kind>`.
- Each impl uses `write!` into a `String` with `LossyByteWrite` for
  the 256-byte cap (truncate, don't wrap). Truncation produces a
  trailing `…`.

## Acceptance

- Unit tests per kind assert the snapshot matches a fixture string
  (modulo path placeholders).
- Each snapshot is ≤ 256 bytes for representative inputs.
- The preamble assembler picks up new snapshots through the
  registry — no per-kind code in the assembler.
- `cargo test -p codon-agent` passes.
