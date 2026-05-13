---
id: TASK:phase-8/fm-make-symlink
type: task
status: accepted
version: 0.0.1
summary: >
  `ln` chord — create symlinks in current_dir pointing at marked
  targets (or the cursor's target with no marks).
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/fm-symlinks#c-make-symlink
---

# File-manager create symlink

## What ships

`l`-then-`n` chord (no conflict with the existing `l` =
enter-directory since the chord state expects a second key). For
each marked target (or cursor entry):

1. Compute link name = target's basename. Apply numbered-suffix
   conflict resolution (`foo` → `foo (2)`) — reuse
   `next_available_path` from the phase-5 paste code.
2. Call `fs::Fs::create_symlink(target, link_in_current_dir)`. If
   the `Fs` trait doesn't expose `create_symlink` yet, add it —
   matches the additive-method pattern used for `copy` / `rename`.

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
chord dispatch + `Fs` trait extension. ~80 LOC.
