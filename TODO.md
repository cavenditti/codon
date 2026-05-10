# Codon roadmap

Codon's roadmap now lives in `.specs/` using the
[forge-spec](https://github.com/daedal-one/forge-spec) format (vendored
at `vendor/forge-spec/`) extended with a codon-local `TASK` entity type
that carries an implementation-lifecycle `progress:` field.

## Quick orientation

- **What's open right now?** `spec todo`
- **What's open in Phase 2?** `spec todo --under TOPIC:topics/phase-2`
- **Coverage for a feature?** `spec coverage REQ:codon/sessions`
- **Working with specs from an agent?** see `.specs/AGENTS.md`

The `spec` binary is at
`vendor/forge-spec/spec-cli/target/release/spec`.

## Phase status (high-level)

| Phase                                                       | Status     |
|-------------------------------------------------------------|------------|
| [Phase 1 — modal shell](spec:TOPIC:topics/phase-1)          | accepted   |
| [Phase 2 — sessions, layout, persistence](spec:TOPIC:topics/phase-2) | accepted (with deferred items) |
| [Phase 3 — agent, inline, commit](spec:TOPIC:topics/phase-3)| accepted (with deferred items) |
| [Phase 4 — buffer trait & git](spec:TOPIC:topics/phase-4)   | draft      |
| [Phase 5 — native UX coverage](spec:TOPIC:topics/phase-5)   | draft      |

Per-clause coverage and per-task progress are tracked in the spec tree;
this file is intentionally short.
