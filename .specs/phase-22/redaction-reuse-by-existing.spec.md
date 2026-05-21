---
id: TASK:phase-22/redaction-reuse-by-existing
type: task
status: accepted
version: 0.1.0
summary: >
  Refactor `memory-secret-redaction` and `preamble-secret-redaction`
  call sites to route through the shared `codon_redact` pipeline.
  Add the architecture test that prevents future code from
  bypassing the pipeline for LLM-bound bytes.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/secret-redaction-pipeline#c-caller-list
aspects: [refactor-callers]
blocked_by:
  - TASK:phase-22/redaction-pipeline-trait
  - TASK:phase-22/preamble-secret-redaction
  - TASK:phase-22/memory-secret-redaction
---

# Reuse the pipeline from existing redaction call sites

## Plan

- Refactor:
  - `memory-secret-redaction` (sibling task already shipped) —
    swap its inline pattern + entropy implementation for a
    single call to `codon_redact::default_pipeline().redact()`.
    The behaviour widens (now also catches Presidio's NER
    matches) but the public API of
    `MemoryStore::validate_body` stays the same: `Ok(())` on
    Clean, `Err(RedactionReason)` on Redacted or Risky.
  - `preamble-secret-redaction` — replace the inline
    `is_secret_name` + entropy check with a pipeline call on
    each snapshot's text. The preamble path keeps its hard
    perf budget; ModelStage is skipped here because the
    preamble runs inside the < 5 ms budget. Skip is via a
    `[redaction] preamble_model_stage = false` config default
    (configurable for users who want the heavier scrub).
- Architecture test:
  - New test file `crates/codon-agent/tests/no_raw_llm.rs`.
  - Walks every Rust source file under `crates/` and asserts
    no file outside `codon-redact` calls into the model client
    while passing a `String` that didn't go through the
    pipeline. Implementation: a regex-grep for known model-call
    sites (`Harness::run_turn`, `ModelClient::complete`,
    etc.) followed by an AST walk of the caller's argument
    chain to ensure it's typed `RedactedText`.
  - On regression: clear failure message naming the offending
    callsite.
- A migration note: any future LLM-call surface added to codon
  must use `RedactedText` for body arguments. The compiler
  enforces this since `RedactedText` has no public constructor.
  The architecture test is the secondary guard against creative
  workarounds (e.g. `String::from(my_text)` cast paths).

## Acceptance

- The two existing tasks' call sites now use the pipeline.
  Their unit tests still pass.
- The architecture test passes on the current tree and fails
  when a deliberately-injected raw-string LLM call is added.
- `cargo test -p codon-memory && cargo test -p codon-agent`
  passes.
- A redaction event for `caller = "memory_remember"` appears in
  the trace when `remember` is called with a tainted body.
- A redaction event for `caller = "preamble"` appears in the
  trace for every preamble build (since pattern + entropy run
  unconditionally there).
