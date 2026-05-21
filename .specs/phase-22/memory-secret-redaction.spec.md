---
id: TASK:phase-22/memory-secret-redaction
type: task
status: accepted
version: 0.1.0
summary: >
  Reuse the secret-pattern list from the preamble REQ to validate
  memory bodies before write. Matched bodies surface a redaction
  reason; the user edits in-place and reconfirms.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-shared-memory#c-no-secrets
---

# Memory secret redaction

## Plan

- `MemoryStore::validate_body(body: &str, patterns: &[String]) ->
  Result<(), RedactionReason>` — uses the same
  `codon_agent::redact::is_secret_match` helper added by sibling
  task `preamble-secret-redaction` (extended here with body-content
  scanning, not just env-var names).
- Two checks:
  1. Substring scan for the env-name patterns (`AWS_SECRET_ACCESS_
     KEY=...` style assignments).
  2. High-entropy scan for sequences of `[A-Za-z0-9+/]{32,}` —
     matches base64 keys, JWTs, etc. False positives are accepted;
     the confirm-overlay lets the user override after editing.
- Hook into `remember` (sibling task `memory-tools`): on a match,
  the tool returns `redaction_required { reason }` and the overlay
  does *not* open. The user can call the tool again with an
  edited body; iterating is the agent's job.
- Picker `c` create flow also validates on save. If a hand-typed
  memory hits the redactor, show the editor pane's diagnostics
  with the matched range — same path Zed uses for lint errors.

## Acceptance

- `MemoryStore::validate_body` returns `RedactionReason::EnvKeyName`
  for `AWS_SECRET_ACCESS_KEY=...` and `RedactionReason::HighEntropy`
  for a 64-char base64 string.
- A clean body returns `Ok(())`.
- The `remember` tool path is covered by an integration test:
  agent calls with a tainted body → `redaction_required` returned;
  no file written.
- `cargo test -p codon-memory` passes.
