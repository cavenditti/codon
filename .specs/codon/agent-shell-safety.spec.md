---
id: REQ:codon/agent-shell-safety
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Harness shell commands pass a layered safety pipeline — deterministic
  hard-deny/secret/allowlist gates, user TOML permission rules, a
  structured-JSON classifier consult with two-model deny escalation and
  a fail-safe `ask` — and approved commands actually execute with
  cancellation, output caps, and metadata-only tracing.
owners: [carlo]
refines:
  - REQ:codon/agent-routing-harness#c-shell-safety
categorized_under: []
---

# Agent shell safety

## Context

The routing harness gates scripted shell tools through a configured
safety agent (REQ:codon/agent-routing-harness#c-shell-safety), but the
current gate is a single-model ALLOW/DENY line-protocol consult and the
tool never executes anything — it is approval-only scaffolding.

The reference design is Carlo's opencode guarded-bash plugin
(`~/.config/opencode/plugin/bash.ts`, versioned at
github.com/cavenditti/opencode-config): deterministic layers run before
any model call, the classifier returns a structured verdict, an LLM
deny is never final on its own, and the fail-safe decision is `ask`,
never `allow`. This REQ ports that architecture into
`codon_agent::runtime` as native Rust, replacing the line-protocol
consult, and completes the tool with real execution.

Decisions are three-way: `allow` (execute), `deny` (refuse), `ask`
(defer to the user). Only the deterministic layers may hard-block; model
verdicts can at most refuse-and-escalate.

:::{requirement id="agent-shell-safety" level="MUST"}
- {#c-deterministic-gates} Before any model consult, a shell command
  MUST pass deterministic gates evaluated in order: (1) hard-deny
  patterns for irreversible system damage (`rm` against filesystem
  root, `mkfs`/`wipefs`, `dd` onto block devices, fork bombs) — a
  hard-deny refuses immediately and can NEVER be overridden by a model
  verdict, a user permission rule, or fail-open mode; (2) secret-deny
  patterns (credential/key/env-file references, ssh/aws/gnupg/kube
  material) with the same never-overridable contract; (3) a
  shell-metacharacter gate and a path gate (parent-directory traversal,
  absolute paths) that route the command past the static allowlist
  straight to classification; (4) a safe-command allowlist of read-only
  shapes (`git status`, `ls`, `cat`, `rg`, …) that short-circuits to
  `allow` without a model call.
- {#c-permission-rules} `codon.toml` MUST accept user shell permission
  rules under `[agent_harness]` — ordered `pattern → allow|ask|deny`
  entries, last matching rule wins — evaluated after the hard-deny
  layers and before the metacharacter gate. A user `deny` refuses
  immediately; a user `allow`/`ask` cannot override a hard-deny.
- {#c-structured-verdict} The classifier consult MUST use a structured
  JSON verdict — `{decision: allow|ask|deny, risk: 0-100, categories:
  [...], reason}` — with the contract carried in the tool-side prompt
  (a flow-authored agent prompt can tune tone, never weaken the
  contract). Replies are parsed leniently (fence-stripped,
  brace-extracted) then shape-validated. An invalid shape, timeout, or
  unavailable classifier resolves to the fail-safe decision `ask`,
  never `allow`.
- {#c-intent} The shell tool MUST accept an optional model-stated
  `description` (intent), surfaced to the classifier as weak untrusted
  evidence. Intent MUST NOT be able to launder hard-deny categories.
  The trace records intent presence only, never its bytes.
- {#c-deny-escalation} A classifier `deny` MUST NOT be final on its
  own. With an escalation agent configured
  (`safety_for("shell", primary, escalation)` in the flow), the deny is
  re-examined by the escalation agent: the command is allowed only when
  the second opinion allows, neither pass flagged a sensitive category
  (destructive / irreversible / secret / credential / exfiltration /
  privilege), and second-opinion risk < 50 — anything else resolves to
  `ask`. Without an escalation agent, a classifier deny resolves to
  `ask`.
- {#c-ask-decision} `ask` MUST resolve through a keyboard-first,
  one-shot user approval (no persistent allowlist) once the approval
  overlay ships; until then `ask` fails closed to a refusal whose
  reason names the pending overlay task. `[agent_harness]
  shell_safety_fail_open = true` MAY collapse `ask` to `allow` for
  development, but MUST NOT bypass the hard-deny layers.
- {#c-execution} An `allow` decision MUST execute the command: `sh -c`
  (never the user's interactive shell), honoring the requested `cwd`,
  killed on cancellation, combined stdout+stderr byte-capped with an
  explicit truncation marker, and the exit code always reported to the
  model.
- {#c-safety-trace} Every decision MUST reach the trace as metadata —
  final decision, deciding layer (hard-deny / secret / rule / safelist
  / classifier / escalation / fail-safe), risk, categories, escalated
  flag — and never command bytes, extending
  REQ:codon/agent-routing-harness#c-monitoring.
- {#c-tests} The gate layers, permission rules, verdict parsing,
  escalation matrix, fail-safe paths, and execution (round-trip,
  cancellation kill, output cap) MUST be covered by no-network tests
  with stub model clients.
:::
