---
id: TASK:phase-5/rewire-clone-destination
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the OS-native git clone destination dialog at
  git_ui/clone.rs:17 with the DirPicker-backed in-app modal.
owners: [carlo]
progress: done
refines:
  - REQ:codon/in-app-pickers#c-rewire-clone
---

# Rewire: git clone destination

## Callsite

[`vendor/zed/crates/git_ui/src/clone.rs`](spec:src:vendor/zed/crates/git_ui/src/clone.rs)
line ~17 — "Select as Repository Destination" prompt.

## Approach

Replace the OS dialog with a DirPicker modal. Pass the selected path
into the existing clone-flow callback (which then runs `git clone
<url> <path>` via the git crate).

Prereq:
[TASK:phase-5/dir-picker-delegate](spec:TASK:phase-5/dir-picker-delegate).

~60 LOC. Smaller than the others because the clone flow already
wraps the path in a single callback closure.
