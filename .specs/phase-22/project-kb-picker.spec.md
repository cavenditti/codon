---
id: TASK:phase-22/project-kb-picker
type: task
status: accepted
version: 0.1.0
summary: >
  `codon_project_kb::OpenPicker` modal listing every directory +
  project summary. Enter opens the summary in a read-only buffer;
  `r` triggers an immediate refresh; `d` deletes the row so the
  next refresh rebuilds.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/project-knowledge-base#c-picker
aspects: [picker-modal]
blocked_by:
  - TASK:phase-22/project-kb-aggregator
---

# Project KB picker

## Plan

- New module `crates/codon-project-kb/src/picker.rs` built on
  [`codon-pickers::ModalScaffold`](spec:src:crates/codon-pickers/src/scaffold.rs)
  + Zed's `picker::Picker`.
- Layout:
  - Pin the project summary row at the top with a `📁 project`
    glyph (only if the user has the unicode-glyphs preference on;
    otherwise plain `[project]`).
  - Below: directory summaries sorted by absolute path.
  - Columns: `scope`, `path` (rendered relative to workspace
    root), `generated` (relative time), `tokens`.
- Bindings inside the picker:
  - `enter` — open the summary in a new editor pane (read-only)
    with the markdown rendered.
  - `r` — call `Aggregator::refresh_*` synchronously for the
    highlighted row. The picker shows a spinner during the call;
    refreshes the row on completion.
  - `d` — delete the row. Confirm with `y/n`. Next refresh
    rebuilds.
- Add `"prefix shift-h" = "codon_project_kb::OpenPicker"` to
  embedded defaults (immediately neighbouring `prefix h` for the
  history picker — pairing them by chord keeps muscle memory
  consistent).
- Opt-in gate: when project-kb is disabled the picker opens with
  an empty state plus a hint at the config knob.

## Acceptance

- `cmd-k shift-h` opens the picker with all summaries.
- `r` triggers a refresh; spinner appears; row updates on
  completion.
- `d` + `y` removes the row; the next scheduled refresh rebuilds.
- Enter opens a read-only buffer with the rendered markdown.
- Disabled state: picker opens with empty + hint.
- `cargo test -p codon-project-kb` passes.
