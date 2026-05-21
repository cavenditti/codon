---
id: TASK:phase-22/redaction-pipeline-trait
type: task
status: accepted
version: 0.1.0
summary: >
  Create `crates/codon-redact/` with the `Redactor` trait, the
  staged composition (`PatternStage -> EntropyStage ->
  ModelStage`), the `RedactionOutcome` types, the fail-closed
  contract, and the per-stage performance budget.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/secret-redaction-pipeline#c-pluggable-trait
  - REQ:codon/secret-redaction-pipeline#c-three-stages
  - REQ:codon/secret-redaction-pipeline#c-fail-closed
  - REQ:codon/secret-redaction-pipeline#c-redaction-token
  - REQ:codon/secret-redaction-pipeline#c-no-remote-by-default
  - REQ:codon/secret-redaction-pipeline#c-perf-budget
  - REQ:codon/secret-redaction-pipeline#c-test-corpus
aspects: [trait-shape, three-stages, fail-closed, redaction-tokens, remote-gate, perf-bench, corpus-test]
blocked_by:
  - TASK:phase-22/redaction-evaluate-models
---

# Redactor trait + staged pipeline

## Plan

- New crate `crates/codon-redact/` with lib-root
  `src/codon_redact.rs`.
- Types:
  ```rust
  pub trait Redactor: Send + Sync {
      fn redact(&self, text: &str, cx: &RedactCtx) -> RedactionOutcome;
  }
  pub struct RedactCtx { pub caller: &'static str, pub timeout: Duration }
  pub enum RedactionOutcome {
      Clean(String),
      Redacted { text: String, spans: Vec<RedactionSpan> },
      Risky { reason: RiskyReason },
  }
  pub struct RedactionSpan { pub start: usize, pub end: usize, pub kind: SpanKind }
  pub enum RiskyReason { StructuralSensitive, StageTimeout, StageError, TooSmallHighEntropy }
  ```
- The `RedactedText` newtype lives here and is publicly
  non-constructible outside this crate (no public ctor; only
  `redact` returns it). Its purpose is to be the *only*
  argument type accepted by LLM-call sites — the harness's
  `ModelClient::complete` takes `RedactedText`, not `String`.
  Storage layers, picker rendering, and PTY paste paths
  continue to use plain `String`; the newtype is the egress
  gate, not a persistence constraint.
- Default pipeline:
  ```rust
  pub fn default_pipeline() -> impl Redactor {
      StagedPipeline::new()
          .with_stage(PatternStage::default())
          .with_stage(EntropyStage::default())
          .with_stage(ModelStage::default()) // Presidio sidecar
  }
  ```
- Stages:
  - **PatternStage**: env-name globs + signature literals.
    Implementation reuses
    `codon_agent::redact::is_secret_name` (extracted from the
    preamble crate).
  - **EntropyStage**: sliding-window Shannon entropy over
    `[A-Za-z0-9+/=_-]` runs of length ≥ 16. Threshold
    configurable.
  - **ModelStage**: backend = the choice from
    `redaction-evaluate-models`. Presidio implementation
    starts a Python sidecar lazily and pipes input via stdin /
    JSON. Sidecar manage logic lives in
    `crates/codon-redact/src/sidecar.rs`.
- Composition: each stage produces spans. Stages run in
  order; later stages see the *original* text but their spans
  are unioned with earlier stages. Final replacement happens
  once at the end, longest-span-wins for overlaps.
- Fail-closed contract: any stage returning
  `Risky` short-circuits the pipeline; later stages do not run;
  the pipeline returns `Risky`. A stage timeout (the per-stage
  `RedactCtx::timeout` default 2 s) maps to
  `RiskyReason::StageTimeout`.
- Remote backend gate: `[redaction] allow_remote_model` config
  flag. Default false. A non-default ModelStage that requires
  network is silently downgraded to Presidio when the flag is
  false; a one-time toast informs the user on workspace open.
- Perf budget: pattern + entropy stages MUST complete in < 1 ms
  for 64 KiB input. Criterion bench at
  `crates/codon-redact/benches/perf.rs`.
- Corpus test: a confusion-matrix test against the fixture
  corpus from `redaction-evaluate-models`. Asserts recall ≥
  documented baseline per kind; precision ≥ documented
  baseline. CI fails on regression.

## Acceptance

- `default_pipeline().redact(...)` round-trips a clean string as
  `Clean`.
- A string containing `AWS_SECRET_ACCESS_KEY=AKIA...` returns
  `Redacted` with the value replaced by `<REDACTED:aws_key>`.
- A multi-line PEM private key returns `Risky` with
  `StructuralSensitive`.
- A stage timeout returns `Risky` with `StageTimeout`.
- Criterion: pattern+entropy stages p99 < 1 ms on 64 KiB.
- Confusion matrix test passes the documented thresholds.
- `cargo test -p codon-redact` passes.
