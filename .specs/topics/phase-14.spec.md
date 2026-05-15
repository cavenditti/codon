---
id: TOPIC:topics/phase-14
type: topic
status: draft
version: 0.0.1
summary: >
  Codebase hygiene & consistency pass. Eliminate `unwrap()` / silent
  `let _ =` in production code, extract a shared modal scaffold,
  unify `CodonModeTracker` updates through a single pane-bridge
  trait, decouple `codon-keymap` from downstream codon crates,
  collapse the two parallel registries in `workspace::codon_bridge`,
  raise the test-coverage floor in the currently untested codon
  crates, and resolve doc drift in CLAUDE.md and
  codon-architecture.typ.
owners: [carlo]
---

# Phase 14 — Codebase hygiene & consistency pass

Phases 1–13 grew codon from a single modal pane into a multi-window
session multiplexer with cross-pane agent verbs, a yazi-style file
manager, jump hints, a command palette, and (in flight) a three-zone
status bar. That growth was feature-shaped, not hygiene-shaped. An
audit of the current tree surfaced ~30 concrete findings in 16
categories. The most load-bearing:

- **CLAUDE.md invariants are violated.** "No `unwrap()`" and "no
  silent `let _ =`" are stated rules; the tree has ~495
  `unwrap()`/`expect()` call-sites (325 in `apps/codon/src/zed.rs`
  alone — Zed's adapted main, but still ours) and 21 `let _ =`
  discards in non-test code. Several silent discards are on the
  keymap / settings load path, where a parse failure today goes
  undiagnosed.
- **Modal and picker scaffolding is copy-pasted** across five
  crates: same `focus_handle` + `EventEmitter<DismissEvent>` +
  `new(cx)` shape reimplemented in cheatsheet, command-palette,
  session picker, window picker, and dir picker.
- **`CodonModeTracker` updates diverge** per pane kind. The
  command-palette flips `command_active` via a global update, the
  file manager updates on focus, the cheatsheet doesn't touch the
  tracker at all. No canonical "pane focus → mode" hook exists.
- **`codon-keymap` over-couples** to five downstream codon crates
  (agent, command-palette, config, jump, session) because action
  registration lives there instead of in each crate's own `init`.
- **`workspace::codon_bridge`** has two parallel registry patterns
  (function-pointer `OnceLock` + `HashMap` of closures) that could
  be one.
- **Test coverage is uneven.** `file-manager` and `codon-jump` are
  well covered; `codon-agent`, `codon-pickers`, `codon-panes`,
  `codon-session/actions`, and the cheatsheet modal have no tests.
- **`spec lint` has 9 errors** — historical commits whose
  `Spec-Ref:` trailers point to ids that no longer exist (phase-5
  era renames).

The outcome we want: codon's invariants are not just documented in
CLAUDE.md, they are enforced by the structure of the code itself.

## Mental model

A single REQ (`REQ:codon/code-quality`, bumped to v0.0.2) defines
the discipline. Each refining TASK is one work-stream / one branch
/ one merge-from-worktree, ordered for minimum churn:

1. Modal scaffold extraction (unblocks every modal/picker downstream).
2. PaneModeBridge trait (unblocks consistent mode dispatch).
3. Robustness sweep — kill `unwrap()` / `expect()` in production code.
4. Silent discard sweep — kill `let _ = <fallible>` in production code.
5. Keymap decoupling — trim `codon-keymap` dependency tree.
6. `codon_bridge` single-registry collapse.
7. Error-pattern documentation per crate.
8. Dead code & silencer-function purge.
9. Naming consistency sweep.
10. Test coverage floor uplift in untested codon crates.
11. Doc drift resolution (CLAUDE.md, codon-architecture.typ, keymap.example.toml).
12. `spec lint` stale-Spec-Ref housekeeping.

Refining requirement:

- [REQ:codon/code-quality](spec:REQ:codon/code-quality) v0.0.2 —
  clauses `#c-no-unwrap-in-codon`, `#c-no-silent-discards`,
  `#c-modal-scaffolding-shared`, `#c-mode-dispatch-hook`,
  `#c-keymap-decoupled`, `#c-error-pattern-per-crate`,
  `#c-no-silencer-functions`, `#c-codon-bridge-single-registry`,
  `#c-naming-consistency`, `#c-doc-drift-check`, plus continued
  refinement of the existing `#c-test-coverage-floor`.
