---
id: TASK:phase-5/rewire-project-panel-download
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the OS-native download-destination dialog at
  project_panel.rs:3301 with the DirPicker-backed in-app modal.
owners: [carlo]
progress: done
refines:
  - REQ:codon/in-app-pickers#c-rewire-project-panel
---

# Rewire: project panel download destination

## Callsite

[`vendor/zed/crates/project_panel/src/project_panel.rs`](spec:src:vendor/zed/crates/project_panel/src/project_panel.rs)
line ~3301 — `cx.prompt_for_paths(...)` for picking the destination
directory when downloading a remote file.

## Approach

Replace the OS dialog with a DirPicker opened as a workspace modal
configured for "directory-only" selection. Wire the `DirSelected`
event into the existing download-completion path.

Prereq:
[TASK:phase-5/dir-picker-delegate](spec:TASK:phase-5/dir-picker-delegate).

~60–80 LOC.
