---
id: TASK:phase-22/preamble-budget-determinism
type: task
status: accepted
version: 0.1.0
summary: >
  Enforce the 2 KiB byte budget with priority-ordered section drops,
  guarantee byte-identical output for identical pane state, and meet
  a < 5 ms warm-build performance target.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-context-preamble#c-byte-budget
  - REQ:codon/agent-context-preamble#c-deterministic
  - REQ:codon/agent-context-preamble#c-cheap-to-build
aspects: [byte-budget, determinism, benchmark]
---

# Preamble budget, determinism, performance

## Plan

- Add `[agent_preamble]` table to `codon-config`:
  - `byte_budget = 2048` (default).
- The assembler tracks a remaining-budget counter and drops
  sections in *reverse priority* once a section's body would push
  past the cap:
  1. Memories (drop first).
  2. Selection detail (keep the one-line summary; drop scrollback
     hints).
  3. Pane scrollback hints (kind-specific snapshot extras).
  4. Never drop: header, workspace/session/window, pane-kind line,
     core pane snapshot.
- Determinism guards:
  - Sections sort their contents (e.g. memory titles) by lexical
    key, not by hashmap iteration.
  - No timestamps. No process IDs. No `Instant::now`.
  - Property test: build the preamble twice in succession with
    identical fixture state — assert byte equality.
- Performance:
  - Criterion benchmark at `crates/codon-agent/benches/preamble.rs`
    that builds the preamble against a synthetic workspace.
  - Target: p50 < 1 ms, p99 < 5 ms on the developer laptop spec
    documented in CLAUDE.md.
- The benchmark is added to CI gating but the budget itself is
  enforced at build-time (panic-free; truncation is graceful).

## Acceptance

- Property test passes: same fixture state → byte-identical output
  across 100 iterations.
- Budget test: when memories would push the preamble past 2 KiB,
  memories drop first; the core sections remain.
- Criterion run reports p99 < 5 ms locally.
- `cargo test -p codon-agent` passes; `cargo bench -p codon-agent
  preamble` runs.
