---
id: TASK:phase-22/harness-evaluate-forge
type: task
status: accepted
version: 0.1.0
summary: >
  Evaluate https://github.com/antoinezambelli/forge as the host
  harness library. Land a one-page memo at
  `docs/decisions/0001-agent-harness.md` plus a working spike that
  drives one pane-tool through forge. Memo is the gate before any
  irreversible adoption.
owners: [carlo]
progress: done
refines:
  - REQ:codon/agent-harness#c-evaluate-forge
---

# Forge evaluation memo + spike

## Plan

- Read forge's README, public API docs, and the most recent
  release notes.
- Build a throwaway spike under `examples/agent-harness-forge/`:
  - One tool: `grep_current_pane` returning a hard-coded `Vec<
    SearchHit>`.
  - One turn driver that calls a stub LLM client (no real model
    calls).
  - Asserts the tool was invoked and the result was surfaced as
    `TurnOutcome::Suggestion(SuggestResponse)`.
- Write the memo at `docs/decisions/0001-agent-harness.md`
  covering:
  - **API shape:** does forge's `Tool` trait compose with codon's
    GPUI-rooted closures (need `&mut AsyncApp` + `Task<Result<...>>`)?
    Concrete code sample either way.
  - **Runtime compatibility:** GPUI is single-threaded on the main
    loop; forge's executor expectations.
  - **Dependency footprint:** crate count, build time delta on a
    `cargo build -p codon` baseline.
  - **License:** confirm compatibility with codon's license posture.
  - **Maintenance signal:** last release, commit cadence, issue
    triage state.
  - **Recommendation:** adopt as a crates.io dep / vendor /
    in-house instead. Justify in two paragraphs.
- The memo MUST end with an unambiguous recommendation. A
  "needs more research" outcome means rerun the task — do not
  hand-wave into adoption.

## Acceptance

- The memo exists at `docs/decisions/0001-agent-harness.md` and
  ends with a clear adopt/decline recommendation.
- The spike under `examples/agent-harness-forge/` compiles and
  runs (`cargo run --example agent-harness-forge`).
- The PR comment summary lists the three biggest API friction
  points (or "no friction").
- `spec lint` clean.
