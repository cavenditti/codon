---
id: TASK:phase-5/dir-picker-delegate
type: task
status: accepted
version: 0.0.1
summary: >
  Reusable DirPicker PickerDelegate that lists directories from a
  starting path with type-to-filter and h/l for navigation.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/in-app-pickers#c-picker-delegate
---

# DirPicker reusable delegate

## What ships

A new module `crates/codon-pickers/` (or a sub-module in
`codon-session` since it already has picker scaffolding) exposing:

- `DirPicker` — a `PickerDelegate` modelled on
  [`crates/codon-session/src/picker.rs`](spec:src:crates/codon-session/src/picker.rs)
  's `SessionPickerDelegate`.
- `DirPickerModal` — the `ModalView` wrapper.

Behaviour:

- Lists entries under a starting directory.
- Type-to-filter via `fuzzy::match_strings`.
- `l` / `Enter` on a directory: descend into it.
- `h`: ascend to parent.
- Confirm emits `DirSelected(PathBuf)`.

## Where it comes from

- Picker infrastructure:
  [`vendor/zed/crates/picker/src/picker.rs`](spec:src:vendor/zed/crates/picker/src/picker.rs).
- Template:
  [`crates/codon-session/src/picker.rs`](spec:src:crates/codon-session/src/picker.rs).
- Secondary reference:
  [`vendor/zed/crates/recent_projects/src/wsl_picker.rs`](spec:src:vendor/zed/crates/recent_projects/src/wsl_picker.rs).

## Approach

Copy the SessionPickerDelegate skeleton, replace the candidate source
(Vec<Session> → directory entries from `fs::read_dir`), add the
`l`/`h` directory navigation. Each rewire task below builds its own
modal around this delegate (or just toggles it directly via
`workspace.toggle_modal`). ~150–200 LOC for the delegate + modal pair.
