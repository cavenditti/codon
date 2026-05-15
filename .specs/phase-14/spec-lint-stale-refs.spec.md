---
id: TASK:phase-14/spec-lint-stale-refs
type: task
status: draft
version: 0.0.1
summary: >
  Address the 9 historical `R013` `spec lint` errors — commits
  whose `Spec-Ref:` trailers point to ids that no longer exist
  (phase-5 era renames). Pick one of three resolutions and
  document the choice in `.specs/AGENTS.md`.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/code-quality#c-spec-lint-clean
---

# Resolve the 9 stale `Spec-Ref:` lint errors

## What changes

`spec lint` on the current tree reports 9 `R013` errors:

```
error[R013]: commit cea6ef9 has Spec-Ref to 'TASK:phase-N/terminal-scrollbar' which does not exist
error[R013]: commit 05cc9ab has Spec-Ref to 'REQ:codon/window-chrome' which does not exist
error[R013]: commit 423f2f9 has Spec-Ref to 'REQ:codon/file-manager#ui' which does not exist
error[R013]: commit d4ce1f5 has Spec-Ref to 'REQ:codon/keyboard-only-ui' which does not exist
error[R013]: commit 9e2a698 has Spec-Ref to 'REQ:codon/branding' which does not exist
error[R013]: commit 7a8660b has Spec-Ref to 'REQ:codon/file-manager#esc-semantics' which does not exist
error[R013]: commit 763a97e has Spec-Ref to 'TOPIC:phase-6' which does not exist
error[R013]: commit 763a97e has Spec-Ref to 'TOPIC:phase-7' which does not exist
error[R013]: commit 763a97e has Spec-Ref to 'TOPIC:phase-8' which does not exist
```

All 9 are from past commits whose target ids were renamed or never
landed. The lint is correct that those ids don't exist; the
commits are immutable history. Three viable resolutions:

### Option A — wontdo placeholder TASK / REQ files

For each missing id, add a `.spec.md` file with
`status: wontdo` (or `superseded`) documenting the rename. The
lint then resolves the reference. Pro: history preserved
verbatim. Con: clutters `.specs/`.

### Option B — `spec lint --since <hash>` cutoff

Extend `vendor/forge-spec/spec-cli` to support a "validate
commits only since this hash" flag. The cutoff hash is set just
after the latest stale-ref commit. Pro: cleanest going forward.
Con: requires upstream spec-cli work (which is fine — it's
vendored on the `codon` branch and we own it).

### Option C — accept the warnings

Configure `spec lint` to demote `R013` to a warning instead of
an error, OR document that "9 historical R013 errors are
expected" in `.specs/AGENTS.md` and make `spec lint` clean mean
"no NEW errors". Pro: zero churn. Con: every future contributor
re-asks the question.

## Approach

1. **Choose**: TASK author picks the option. Recommendation: **A**
   for ergonomics (no spec-cli changes, no perma-noise).
2. Implement the chosen option:
   - **A**: add 6 placeholder spec files (the 9 errors fold into
     6 unique ids: `TASK:phase-N/terminal-scrollbar`,
     `REQ:codon/window-chrome`, `REQ:codon/file-manager#ui`,
     `REQ:codon/keyboard-only-ui`, `REQ:codon/branding`,
     `REQ:codon/file-manager#esc-semantics`,
     `TOPIC:phase-6/7/8`). Each file's body explains the rename
     and points to the current home.
   - **B**: edit `vendor/forge-spec/spec-cli` to add
     `--since <hash>`; document in `.specs/AGENTS.md`.
   - **C**: edit `.specs/AGENTS.md` documenting the 9 known
     errors; close the loop on contributor onboarding.
3. Document the choice + rationale in `.specs/AGENTS.md`.

## Non-goals

- Not rewriting git history. Commits stay as-is.
- Not changing past TASK ids that already exist; only the
  references-to-missing-ids problem is in scope.

## Files touched

(Depends on the chosen option.)

Option A:
- `.specs/codon/window-chrome.spec.md` (wontdo placeholder)
- `.specs/codon/keyboard-only-ui.spec.md` (wontdo placeholder)
- `.specs/codon/branding.spec.md` (wontdo placeholder)
- `.specs/codon/file-manager.spec.md` — add `c-ui` and
  `c-esc-semantics` clause stubs marked superseded
- `.specs/phase-5/terminal-scrollbar.spec.md` (wontdo placeholder)
- `.specs/topics/phase-6.spec.md`, `phase-7.spec.md`, `phase-8.spec.md`
  — verify they exist (they should already; the error wording is
  `TOPIC:phase-6` not `TOPIC:topics/phase-6` — the lint may be
  flagging an unprefixed ref. Investigate first; if the topics
  ARE present, the fix is simpler — the commit message used a
  shortened form).
- `.specs/AGENTS.md` — document the choice.

Option B:
- `vendor/forge-spec/spec-cli/src/main.rs` and adjacent —
  `--since` flag.
- `.specs/AGENTS.md` — document.

Option C:
- `.specs/AGENTS.md` — document the 9 known-stale errors.

## Verification

- Option A: `spec lint` returns 0 errors (warnings tolerated;
  pre-existing R010 warnings unrelated to this TASK stay).
- Option B: `spec lint --since <hash>` returns 0 errors;
  full-history `spec lint` still shows the 9 (documented).
- Option C: `spec lint` exit code 0 (after demotion) OR
  contributor-facing doc clearly explains the expected 9 errors.
