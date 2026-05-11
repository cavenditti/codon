# Working with codon's specs

## Pre-commit hook

A `pre-commit` hook that runs `spec lint` is provided in
`.githooks/pre-commit`. To enable it (one-time, per clone):

```sh
git config core.hooksPath .githooks
```

It only runs when `.spec.md` or `_redirects.toml` files are part of the
commit, so it's a no-op for unrelated work.



This directory uses the **forge-spec** format (vendored at
`vendor/forge-spec/`) extended with the codon-local `TASK` entity type.
The CLI binary is at `vendor/forge-spec/spec-cli/target/release/spec`.

## What lives where

```
.specs/
  topics/         TOPIC entries — one per phase (phase-1 ... phase-5)
  codon/          REQ entries — feature requirements, organized by area
  phase-2/        TASK entries — leaf work items for Phase 2
  phase-3/        TASK entries — leaf work items for Phase 3
  phase-4/        (created when Phase 4 work begins)
  phase-5/        (created when Phase 5 work begins)
```

## Hierarchy

```
TOPIC:topics/phase-2          (a phase, organizational)
    ↑ categorized_under
REQ:codon/sessions             (a feature with clauses)
    ├ #c-data-model
    ├ #c-create
    └ ...
    ↑ refines
TASK:phase-2/session-new       (a leaf work item with progress)
    progress: pending | in-progress | done | blocked | deferred | wontdo
```

## Common queries

```sh
# What's open right now?
spec todo

# What's open in Phase 2?
spec todo --under TOPIC:topics/phase-2

# Coverage for a specific requirement (with task progress per clause)
spec coverage REQ:codon/sessions

# Where does this spec sit in the graph?
spec children REQ:codon/sessions
spec ancestors TASK:phase-2/session-new
```

## Lifecycle commands

```sh
spec start  TASK:phase-2/foo              # mark in-progress
spec done   TASK:phase-2/foo              # mark done
spec block  TASK:phase-2/foo --on ADR:codon/0001-stack
spec defer  TASK:phase-2/foo              # out of scope for now
spec wontdo TASK:phase-2/foo              # intentionally not implementing
spec reset  TASK:phase-2/foo              # back to pending
```

`deferred` vs `wontdo`:

- **deferred** — "we'll do this later, when the cost/benefit tilts."
  Surfaces in `spec todo --state deferred`. Revisit periodically.
- **wontdo** — "we've decided not to do this; the parent clause stays
  in the REQ for traceability but no work is planned." Excluded from
  coverage denominators, so a clause whose only task is `wontdo`
  reports as covered (not as outstanding work).

## Commit trailers

When a commit implements / tests / touches a spec, add a `Spec-Ref:`
trailer to the commit message:

```
Spec-Ref: TASK:phase-2/session-new (implements)
Spec-Ref: REQ:codon/sessions#c-create (touches)
```

Kinds: `implements`, `refines`, `tests`, `violates`, `touches`
(default).

## Two distinct lifecycles

- `status:` — document lifecycle: `draft | accepted | deprecated | superseded`.
  REQs/TASKs typically sit at `accepted` once authored.
- `progress:` (TASK only) — implementation lifecycle:
  `pending | in-progress | done | blocked | deferred | wontdo`.
  This is what `spec todo` and `spec coverage` report on.

A task can be `accepted` as a document while still being `pending` as
work to do — that is the common case for newly-written tasks.
