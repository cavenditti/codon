---
id: TASK:phase-22/preamble-secret-redaction
type: task
status: accepted
version: 0.1.0
summary: >
  Filter out env vars whose names match the configured secret
  patterns before any pane snapshot can read them. The terminal
  snapshot's cwd remains; an env dump is never part of any snapshot.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-context-preamble#c-no-secrets
---

# Preamble secret redaction

## Plan

- Add `[agent_preamble] redact_env_patterns = [...]` to
  `codon-config` with defaults `["*_TOKEN", "*_KEY", "*_SECRET",
  "*_PASSWORD", "*_API_KEY", "*_PASSPHRASE"]`. Patterns are
  case-insensitive glob suffixes on the env-var name.
- A `codon_agent::redact::is_secret_name(name: &str, patterns:
  &[String]) -> bool` helper, exported from the preamble crate.
  Used everywhere a snapshot is tempted to include env data.
- Audit every `PaneSnapshot` impl: confirm no env dump. The
  terminal snapshot may surface `SHELL` and `TERM` literally (those
  aren't secrets); anything else from env is forbidden.
- Add a unit test that constructs a snapshot context with an
  `AWS_SECRET_ACCESS_KEY` set and asserts the preamble does not
  contain the value or the key name. Test runs against every pane
  kind via a parameterised matrix.
- A second pass scans the rendered preamble for trailing tokens
  matching `[A-Za-z0-9+/]{32,}` (a cheap entropy proxy). If hit,
  emit a `redaction.high_entropy` trace warning (does not modify
  the output — the goal is to detect leaks during dev, not silently
  scrub at runtime).

## Acceptance

- Unit test passes for every pane kind: `AWS_SECRET_ACCESS_KEY` in
  the env does not appear in the preamble.
- A user-provided extra pattern (`*_TOKEN_V2`) in `codon.toml`
  takes effect (covered by an integration test).
- High-entropy guard fires on a synthetic 64-char alphanumeric
  inserted into a terminal snapshot — verifies the dev warning
  path works.
- `cargo test -p codon-agent` passes.
