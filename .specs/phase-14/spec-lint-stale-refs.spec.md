---
id: TASK:phase-14/spec-lint-stale-refs
type: task
status: accepted
version: 0.0.1
summary: >
  Resolve the nine historical `R013` errors that `spec lint` reports
  on master — commits whose `Spec-Ref:` trailers point at ids that
  were renamed or never adopted during the phase-5 cleanup.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-spec-lint-clean
---

# Reconcile stale historical Spec-Ref trailers

## Context

`spec lint` on the 2026-05-15 master tree prints nine `R013` errors:

```
TASK:phase-N/terminal-scrollbar           (commit cea6ef9)
REQ:codon/window-chrome                   (commit 05cc9ab)
REQ:codon/file-manager#ui                 (commit 423f2f9)
REQ:codon/keyboard-only-ui                (commit d4ce1f5)
REQ:codon/branding                        (commit 9e2a698)
REQ:codon/file-manager#esc-semantics      (commit 7a8660b)
TOPIC:phase-6                             (commit 763a97e)
TOPIC:phase-7                             (commit 763a97e)
TOPIC:phase-8                             (commit 763a97e)
```

Two flavours:

- **Typos / renames** — `TOPIC:phase-6` was the commit-message form
  for `TOPIC:topics/phase-6`; the `c-` prefix on file-manager clause
  ids was adopted mid-phase. The canonical ids exist; only the
  trailer is wrong.
- **Never-adopted ids** — `REQ:codon/window-chrome`,
  `REQ:codon/keyboard-only-ui`, `REQ:codon/branding`, and
  `TASK:phase-N/terminal-scrollbar` were placeholder names the
  author used in trailers before authoring (or instead of authoring)
  the REQ. No spec file was ever written; the work landed under
  other clauses or was vendored straight into Zed.

History is immutable. The fix is forward.

## Approach

Two complementary mechanisms, both already supported by
`vendor/forge-spec/spec-cli`:

1. **`.specs/_redirects.toml`** — `[[redirect]] from = ... to = ...`
   entries for the typo/rename cases. The lint resolves through
   redirects when checking `R013`.
2. **Placeholder spec files** with `status: wontdo` /
   `status: superseded` for the never-adopted ids. The body of each
   placeholder explains why it never grew clauses and points at the
   work that actually landed.

## Resolution per id

- `TOPIC:phase-6/7/8` → redirect to `TOPIC:topics/phase-6/7/8`.
- `REQ:codon/file-manager#ui` → add a `{#c-ui}` clause stub to the
  existing `REQ:codon/file-manager`, marked superseded by the
  `fm-chrome` / `fm-enhancements` / `file-manager-theme` REQs;
  redirect `#ui` → `#c-ui`.
- `REQ:codon/file-manager#esc-semantics` → add a
  `{#c-esc-semantics}` clause stub; redirect `#esc-semantics` →
  `#c-esc-semantics`.
- `REQ:codon/window-chrome` → new placeholder spec, `status:
  wontdo`, documenting that the work landed as the `[window]` config
  surface in `codon-config` rather than a standalone REQ.
- `REQ:codon/keyboard-only-ui` → new placeholder spec, `status:
  superseded`, pointing at the "Keyboard-first, always" rule in
  `/CLAUDE.md` as the canonical statement.
- `REQ:codon/branding` → new placeholder spec, `status: wontdo`,
  noting that the rename work was a one-shot menu/title patch
  with no further requirements to capture.
- `TASK:phase-N/terminal-scrollbar` → new placeholder spec under
  `.specs/phase-5/` (the phase the work belonged to), `progress:
  wontdo`, noting that the scrollbar default flipped via a
  vendored-Zed config bump with no codon-side clause needed.

## Out of scope

- Rewriting git history. The trailers stay as-written.
- Renaming any existing TASK/REQ/TOPIC ids. Only additive.
- Extending `vendor/forge-spec/`. The CLI already supports both
  mechanisms.
