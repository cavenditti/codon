---
id: TASK:phase-22/redaction-evaluate-models
type: task
status: accepted
version: 0.1.0
summary: >
  Comparison memo at `docs/decisions/0002-redaction-model.md`
  benchmarking Presidio (default candidate) against at least one
  LLM-based redactor on a fixed test corpus. Memo ends with an
  unambiguous "default backend" recommendation that the
  `ModelStage` ships.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/secret-redaction-pipeline#c-evaluation-memo
  - REQ:codon/secret-redaction-pipeline#c-offline-default
aspects: [eval-memo, offline-default]
---

# Redaction model evaluation memo

## Plan

- Build the fixture corpus first at
  `crates/codon-redact/tests/corpus/`:
  - 5+ positives and 5+ negatives per `KIND` (email, phone,
    aws_key, gcp_key, pem_private_key, bearer_token, jwt,
    iban, credit_card, ssn, generic_high_entropy).
  - Each entry is a fragment of realistic terminal output or
    command text. Hand-curated; no scraping.
- Candidates to evaluate:
  - **Presidio** (default candidate): Apache-2.0, runs via a
    Python sidecar (`presidio-analyzer` + `presidio-anonymizer`).
    Fully offline. Recommended in the REQ as the default.
  - **LLM-as-redactor**: a small local model (the user's
    request mentioned "good models for this specifically" —
    candidates to consider: a guard-style classifier, an NER-
    finetuned 1-3B model, or a dedicated redaction model from
    HuggingFace's `pii` collection). Evaluate one concrete
    representative.
  - **Pattern+entropy alone** (baseline): no model stage.
    Quantifies what model-based redaction is buying us.
- Benchmark dimensions (all reported in the memo):
  - Detection coverage per `KIND` (recall).
  - False-positive rate on negatives (precision).
  - p50 / p99 latency on 1 KiB and 16 KiB inputs.
  - Memory footprint (RSS).
  - Offline support (yes/no).
  - License compatibility (Apache 2 / MIT compatible).
  - Dependency footprint (python sidecar + venv vs.
    `tch-rs`/`candle` etc.).
  - Maintenance signal (last release, commit cadence).
- Memo structure: one section per candidate, then a comparison
  table, then the recommendation paragraph. Memo ≤ 2 pages.
- Recommendation ends with a single line:
  `Default ModelStage backend: <choice>` so a future reader
  doesn't have to interpret hedging.

## Acceptance

- The corpus exists at the documented path with the documented
  shape.
- The memo exists at `docs/decisions/0002-redaction-model.md`,
  covers all three candidates, ends with the unambiguous
  "Default ModelStage backend" line.
- A throwaway benchmark binary at
  `crates/codon-redact/examples/redact_bench.rs` runs each
  candidate against the corpus and prints the numbers the memo
  cites. Reproducible: `cargo run -p codon-redact --example
  redact_bench` regenerates the table.
- `spec lint` clean.
