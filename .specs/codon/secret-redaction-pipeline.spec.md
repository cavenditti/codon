---
id: REQ:codon/secret-redaction-pipeline
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  A staged, pluggable redactor that every byte destined for an LLM
  call passes through. Three stages: (1) name/value regex match
  against the existing pattern list, (2) high-entropy heuristic,
  (3) model-based NER (default: Presidio, fully offline). Returns
  redacted text with `<REDACTED:KIND>` placeholders, or a
  `Risky` signal that fails closed. Reused by command-history,
  project-knowledge-base, shared-memory, and the agent preamble.
owners: [carlo]
refines: []
categorized_under: [TOPIC:topics/phase-22]
---

# Secret redaction pipeline

## Context

### Threat model

This pipeline protects **egress to LLM providers** (local or
remote). It does NOT protect on-disk persistence — codon's stores
sit on the user's machine inside the user's trust boundary, the
same as `~/.zsh_history`, the editor's undo files, or any other
local cache. A user who has decided to install codon has accepted
that codon stores what they type and run.

The line we're protecting is: bytes leaving the codon process into
a model call (the harness's `ModelClient::complete`). That call
might hit a remote API, a sidecar local model, or a future
alternative; in every case the data is leaving the user's direct
control and we want it scrubbed first.

### Why a shared pipeline

The phase-22 features that ship LLM-bound content — command-history
summarization, project knowledge-base rollups, shared-memory body
checks, the agent preamble, the agent-tool return values that the
model sees — each need to scrub sensitive data before it leaves
the process. Three of them already include a clause to do that
work inline; this REQ consolidates the actual scrubbing into one
pipeline so:

- The scrub rule is one rule, defined and tested in one place.
  When a user reports "codon leaked X into a summary", there's one
  function to inspect.
- The rule can evolve. Today's stage 3 (model-based NER) is
  *added* on top of the existing pattern + entropy stages — those
  stages don't disappear; they become the cheap pre-filter. A
  future stage 4 (e.g. context-aware DLP) plugs in without
  touching the callers.
- The fail-closed behaviour is uniform. When the redactor says
  "I can't confidently scrub this", every caller drops the input.
  There is no "well, this caller is fine with low-confidence
  output" carve-out.

The three stages:

1. **Regex pattern match** (existing). Env-name glob list (e.g.
   `*_TOKEN`, `*_SECRET`), plus signature-style patterns
   (`-----BEGIN PRIVATE KEY-----`, `xoxb-`, `AKIA`, etc.). Cheap
   and exact.
2. **High-entropy heuristic** (existing). Sliding window over
   long runs of `[A-Za-z0-9+/=_-]`; flag spans whose Shannon
   entropy crosses a threshold. Cheap; catches randomly-generated
   secrets the patterns miss.
3. **Model-based NER** (new). Default: Microsoft Presidio with
   the spaCy + transformers analyzers — runs fully offline, no
   network call, recognises emails, phone numbers, credit
   cards, IBANs, plus a configurable PII set. An alternative
   "small LLM as redactor" backend is on the table (an
   evaluation task delivers the comparison memo before the
   default is locked in).

Every redactor stage produces *spans* (start, end, kind). The
pipeline merges spans (overlapping spans collapse to the wider
match), replaces each span with `<REDACTED:KIND>`, and returns
the result alongside metadata (`spans_count`, `kinds_seen`,
`outcome: Clean | Redacted | Risky`).

`Risky` is the fail-closed signal:

- The redactor errored or timed out.
- A stage matched a pattern in a way that suggests the input is
  *structurally* sensitive (e.g. an entire file body shaped like
  a TLS private key — even though the BEGIN/END markers are
  redactable, the body itself is high-entropy across many lines,
  which a partial redaction wouldn't fix).
- Configurable: very small inputs (< 16 bytes) whose entropy is
  high are returned `Risky` rather than redacted because the
  signal-to-noise ratio is too low for the surrounding context
  to remain useful after redaction.

Every caller treats `Risky` the same way: don't make the LLM call.
The command-history fail-closed clause and the memory-redaction
flow already match this rule; this REQ is what they call into.

:::{requirement id="secret-redaction-pipeline" level="MUST"}
The pipeline MUST:

- {#c-pluggable-trait} expose
  `pub trait Redactor: Send + Sync { fn redact(&self, text: &str,
  cx: &RedactCtx) -> RedactionOutcome; }` plus the result type
  ```rust
  pub enum RedactionOutcome {
      Clean(String),
      Redacted { text: String, spans: Vec<RedactionSpan> },
      Risky { reason: RiskyReason },
  }
  ```
  Implementations are registered via
  `codon_redact::register_stage(stage)`. The default pipeline is
  the three-stage composition; users can swap or extend stages
  through `codon.toml`
- {#c-three-stages} the default pipeline composes
  `PatternStage -> EntropyStage -> ModelStage` in order. Each
  stage's spans accumulate; the final output redacts every
  reported span. The pattern and entropy stages are pure-Rust;
  the model stage runs through a sidecar process (default:
  Presidio via stdio) so the GPUI main loop is never blocked
- {#c-fail-closed} a stage that errors, times out (default 2 s
  per call, configurable), or returns
  `RiskyReason::StructuralSensitive` causes the pipeline to
  return `Risky`. NO caller may treat a `Risky` outcome as
  "redact what we can and proceed" — the contract is drop the
  input. Tests assert every caller honours this
- {#c-redaction-token} redacted spans are replaced with
  `<REDACTED:KIND>` where `KIND` is the highest-confidence
  classification (`email`, `phone`, `aws_key`, `pem_private_key`,
  `high_entropy`, etc.). The surrounding text remains intact so
  the summarizer can still produce coherent output
- {#c-offline-default} the default ModelStage backend MUST work
  fully offline. Presidio is the chosen default (Apache-2.0,
  active maintenance). A user who wants a different backend
  (e.g. a local LLM-as-redactor) opts in via
  `[redaction] model_backend = "..."` after the
  `redaction-evaluate-models` task ships the comparison memo
- {#c-no-remote-by-default} sending content to a remote model
  for redaction is OFF by default. Enabling it requires
  `[redaction] allow_remote_model = true` and a one-pane
  onboarding (same shape as command-history's). Default
  posture: everything that touches a remote endpoint goes
  through the pipeline first; the pipeline itself never
  reaches out
- {#c-caller-list} the following egress points MUST use this
  pipeline before any text reaches the model:
  command-history summarization prompts,
  project-knowledge-base rollup prompts, the agent preamble's
  assembled string, the `search_command_history` agent tool's
  return value, the `search_memories` tool's return value
  (when memory bodies are LLM-bound), and any future agent-tool
  output that includes user-typed or shell-output content. The
  rule is "what the model sees goes through the pipeline" —
  storage paths (writing memory files, inserting command-history
  rows) are NOT in scope and remain raw. The
  `RedactedText` newtype from `c-pluggable-trait` is the
  type-system gate on the LLM-call argument; an architecture
  test catches creative bypasses
- {#c-evaluation-memo} an evaluation task lands a memo at
  `docs/decisions/0002-redaction-model.md` comparing Presidio
  (default) against at least one LLM-based redactor
  (recommendations welcome — the user's request mentioned a
  class of models specialised for this). The memo covers:
  detection coverage on a fixed test corpus, false-positive
  rate, latency, offline support, license, dependency footprint.
  The memo's recommendation is what the default `ModelStage`
  ships
- {#c-audit-trace} every redaction event is recorded in the
  harness trace
  (REQ:codon/agent-harness#c-trace) as
  `RedactionEvent { caller, stage_counts, spans_total,
  kinds_summary, outcome }`. NO redacted body or original body
  ever lands in the trace — only counts and kind labels.
  A `codon_redact::AuditPicker` action opens a per-session
  audit view so the user can review what's being scrubbed and
  catch over- or under-redaction
- {#c-perf-budget} the pattern + entropy stages MUST complete in
  < 1 ms for inputs up to 64 KiB (criterion benchmark gates
  this). The model stage's latency is workload-dependent;
  callers run it async and respect the harness cancellation
  token
- {#c-test-corpus} a fixture corpus at
  `crates/codon-redact/tests/corpus/` covers each `KIND` with
  at least 5 positive examples and 5 negative (look-alike)
  examples. CI runs a confusion-matrix test and fails the build
  if either recall or precision regresses
:::

## Out of scope

- A user-facing whitelist for "this string is fine to send".
  Phase 22 doesn't ship an exception list; the user's only
  override is editing the `remember`/picker body manually after
  the redactor flags it.
- Differential privacy / noise injection. The redaction is
  span-replace, not perturbation.
- Centralised redactor service. Every stage runs locally
  (with the opt-in remote-model exception).
- Plaintext-secret detection in *files* the user opens. This
  pipeline only scrubs content destined for LLM calls; an
  editor-side "you have a secret in this buffer" warning is a
  separate REQ.
