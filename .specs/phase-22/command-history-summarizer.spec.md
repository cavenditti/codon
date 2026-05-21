---
id: TASK:phase-22/command-history-summarizer
type: task
status: accepted
version: 0.1.0
summary: >
  Async, queued summarization worker. For each pending entry,
  reads the raw row from the store, builds the prompt and runs it
  through the redaction pipeline (the egress point), calls the
  harness, writes `summary_what` + `summary_did` + tags back.
  On a `Risky` redactor outcome the worker calls
  `mark_skipped(id, "risky_redaction")` and never invokes the
  LLM.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-history#c-summarize-async
  - REQ:codon/command-history#c-summary-shape
  - REQ:codon/command-history#c-redact-on-llm-egress
  - REQ:codon/command-history#c-llm-skip-on-risky
  - REQ:codon/command-history#c-no-llm-leak
aspects: [worker-loop, summary-prompt, egress-redaction, risky-skip, no-leak-test]
blocked_by:
  - TASK:phase-22/command-history-store
  - TASK:phase-22/harness-api
  - TASK:phase-22/redaction-pipeline-trait
---

# Async summarization worker

## Plan

- New module
  `crates/codon-command-history/src/summarizer.rs`.
- Job queue: an `mpsc::UnboundedSender<EntryId>` exposed by
  `HistoryStore`. The subscriber (sibling task) enqueues; the
  worker drains.
- Worker:
  - Runs on the harness's async executor (no new runtime).
  - Drains one entry at a time. For each:
    1. Read the entry's raw command + raw output excerpt + cwd
       + exit code from the store.
    2. Build the summarizer prompt body by substituting the raw
       fields into the template (see Prompt below).
    3. **Egress redaction.** Run the assembled prompt through
       `codon_redact::default_pipeline().redact(prompt, ctx
       { caller: "command_history.summarize" })`. The
       redacted string is the LLM-call argument; the raw
       string is dropped.
       - On `Clean` or `Redacted` → continue with the
         redactor's output.
       - On `Risky` → call
         `HistoryStore::mark_skipped(id, "risky_redaction")`,
         emit a `RedactionEvent` trace, and skip the LLM call.
         The row keeps its raw bytes; the picker shows it
         normally; future summarization attempts on the same
         row also skip.
    4. Call `Harness::run_turn` with the redacted prompt and
       a synthetic `SummarizerFlow` shape that locks down the
       available tools (no pane tools; no memory tools; the
       model returns one structured `SuggestStructured { what,
       did, tags }` shape used only by this flow).
    5. Parse the response, validate (≤ 280 chars each, tags
       ≤ 5), and call `HistoryStore::update_summary` to write
       back.
- Prompt template (in
  `crates/codon-command-history/src/summarizer_prompt.md`):
  ```
  You are summarizing a shell command for a searchable
  history. Be precise and terse.

  Command (redacted): {command}
  Working directory: {cwd}
  Exit code: {exit}
  Output excerpt (redacted): {output}

  Return JSON with:
  - what: one sentence, present tense, what this command does
    in general
  - did: one sentence, past tense, what it did this time
    including exit code and any salient stderr/stdout pattern
  - tags: 0-5 short keywords
  Each field ≤ 280 chars.
  ```
- Daily budget guard: before each call, check
  `[command_history] daily_summarize_token_budget`. When
  exhausted, mark the entry `llm_skipped = true` and skip the
  call. Budget resets at UTC midnight.
- Cancellation: the worker honours workspace close — the
  in-flight call's cancel token fires.

## Acceptance

- Integration test with a stub model client returning canned
  summaries: an inserted entry becomes a summarized entry within
  one tick of the executor.
- **No-leak test**: an entry containing
  `AWS_SECRET_ACCESS_KEY=AKIA...` is inserted; the worker
  invokes the redactor (clean span replacement) and the captured
  stub-model input contains `<REDACTED:aws_key>` and does NOT
  contain `AKIA`. The sqlite row still contains the raw bytes.
- **Risky-skip test**: an entry whose body matches the
  pipeline's `StructuralSensitive` rule (PEM private key
  fixture) is inserted; the worker calls `mark_skipped` and
  emits NO model call. The trace contains a `RedactionEvent`
  with `outcome = "risky"`.
- Budget test: after `daily_summarize_token_budget` is exhausted,
  inserts queue entries but no LLM call fires; `llm_skipped` is
  set with `skip_reason = "budget_exhausted"`.
- Cancellation test: workspace close mid-summarization leaves
  the entry's summary NULL (no half-written rows).
- Validation: a malformed model response (missing `did` field)
  triggers a trace warning; the entry stays with NULL summary
  fields and is NOT retried in the same session.
