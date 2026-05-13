---
id: TASK:phase-7/fm-choose-opener
type: task
status: accepted
version: 0.0.1
summary: >
  `O` shows a picker of openers matching the selected entry, plus a
  "Codon (default)" entry. Marked-set semantics: each opener runs
  once per marked entry.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-openers#c-choose-opener
---

# File-manager choose-opener picker

## What ships

`O` (shift-o) shows a `Picker` of every opener whose glob / mime
matches the current entry. Each row: `description (cmd)`. The last
row is always the synthetic "Codon (default)" entry that runs the
current `workspace.open_abs_path` route.

With marks: the chosen opener spawns once per marked entry,
substituting `{path}` (or `{paths}` if the opener declares
multi-path support).

## Where it slots in

Depends on TASK:phase-7/fm-opener-config to know what openers
exist. Picker uses the `codon-pickers` `Picker` shape. ~150 LOC.
