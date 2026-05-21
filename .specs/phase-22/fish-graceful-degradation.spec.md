---
id: TASK:phase-22/fish-graceful-degradation
type: task
status: accepted
version: 0.1.0
summary: >
  Make every fish-plugin feature check `set -q CODON_SOCK` before
  activating. Outside codon: Ctrl-G is not bound, `codon do`
  prints a clean error, prompt-time hooks are no-ops. A user can
  source `codon.fish` from any fish session safely.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-graceful-degradation
aspects: [outside-codon-noop]
blocked_by:
  - TASK:phase-22/fish-plugin-bootstrap
---

# Graceful degradation outside codon

## Plan

- The plugin file (`codon.fish`) has a single top-level guard:
  ```fish
  if not set -q CODON_SOCK
      function codon
          if test "$argv[1]" = "do"
              echo "codon: not running (CODON_SOCK unset)" >&2
              return 1
          end
          # Pass-through to the `codon` binary on PATH if any —
          # the user may have invoked `codon` to mean "launch the
          # editor". The plugin doesn't shadow that.
          command codon $argv
      end
      return  # skip every binding / hook below
  end
  ```
- Below the guard: bindings (`bind \cg ...`), prompt hooks,
  tab-completion definitions. None of these get installed
  outside codon.
- Verification that the plugin re-source path is clean:
  - Sourcing the plugin in a fresh fish session with
    `CODON_SOCK` unset MUST NOT bind any keys, define any
    hooks, or modify the prompt. Asserted by snapshotting
    `bind | wc -l`, `function | wc -l` deltas pre/post.
- The `codon do ...` outside-codon error message includes a
  one-line hint: "run `codon` to launch the editor first, or
  see <docs URL> for setup".
- A regression test runs the plugin under
  `fish -P` (private mode) without `CODON_SOCK` and asserts
  the shell remains usable for typical commands (`ls`, `cd`,
  history navigation). Run as part of CI.

## Acceptance

- Sourcing the plugin outside codon binds zero keys (verified
  by `bind` diff).
- `codon do anything` outside codon exits non-zero with the
  documented stderr message.
- `codon` (no subcommand) outside codon passes through to the
  binary on PATH if present, otherwise produces fish's normal
  "command not found".
- A fish private-session smoke test runs `ls`, `cd`, `↑`,
  `Ctrl-G` without errors and without surprise behaviour.
- `cargo test -p codon-fish` passes.
