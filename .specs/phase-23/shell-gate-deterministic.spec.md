---
id: TASK:phase-23/shell-gate-deterministic
type: task
status: accepted
version: 0.1.0
summary: >
  Deterministic shell-safety layers (hard-deny, secret-deny,
  metacharacter + path gates, safe-command allowlist) plus user TOML
  permission rules, evaluated before any model consult.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/agent-shell-safety#c-deterministic-gates
  - REQ:codon/agent-shell-safety#c-permission-rules
aspects: [deterministic-gates, permission-rules]
assignee:
eta:
blocked_by: []
---

# Shell gate deterministic

## Plan

New `crates/codon-agent/src/runtime/safety.rs` module porting the
deterministic layers of the opencode guarded-bash plugin
(`~/.config/opencode/plugin/bash.ts`) to Rust with the `regex` crate:

- `HARD_DENY` patterns (rm against `/`, `mkfs`/`wipefs`, `dd of=/dev/`,
  fork bomb) → immediate refusal, `SafetySource::HardDeny`, never
  overridable by rules, classifier, escalation, or fail-open.
- `SECRET_DENY` pattern (env/pem/key/pfx/keystore/netrc/npmrc, ssh/aws/
  gnupg/kube material, auth stores, `/etc/shadow`) → same contract.
- User permission rules from `[agent_harness.shell_permissions]` in
  codon.toml — ordered `{ pattern, decision }` entries, glob-lite
  matching (`*` wildcard), last match wins. `deny` refuses; `allow` /
  `ask` resolve immediately (but never above the hard layers).
- Metacharacter gate + path gate (`..`, absolute path) → mark the
  command classifier-required (skip the allowlist).
- `SAFE_COMMANDS` read-only allowlist (git status/log/diff/show/…, ls,
  cat, head, tail, wc, grep, rg, file, stat, which, pwd, whoami,
  version probes) → `allow` without a model call.

The module exposes `deterministic_verdict(command, rules) ->
Option<SafetyVerdict>` where `None` means "fall through to the
classifier", mirroring the reference semantics.

## Acceptance

Unit tests port the reference corpus: every hard-deny/secret example
refuses with the right source; allowlist shapes allow; metachar/path
commands fall through; permission rules honor last-match-wins and
cannot override hard-deny. `cargo test -p codon-agent` green.
