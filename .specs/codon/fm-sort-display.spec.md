---
id: REQ:codon/fm-sort-display
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Sort modes, line modes, gitignore toggle, preview-pane ratio — the
  display knobs yazi exposes via the `,` and `z` / `M` / `<` `>` chord
  families.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-6]
---

# File manager sort and display

:::{requirement id="fm-sort-display" level="SHOULD"}
The file manager SHOULD support:

- {#c-sort-modes} a `SortMode` enum on `FileManager` with variants
  `Name`, `Size`, `Mtime`, `Btime`, `Extension`, `Random`, `Natural`.
  Selected via `,n` / `,s` / `,m` / `,b` / `,e` / `,r` / `,N`. The
  active mode is persisted to the codon settings file so it survives
  restarts.
- {#c-sort-reverse} `,,` toggles the sort direction. Persisted with
  the mode.
- {#c-line-modes} a `LineMode` enum cycled with `M` — `None` (today's
  behavior), `Size`, `Mtime`, `Permissions`, `Owner`. The chosen mode
  appends a right-justified metadata column to each row.
- {#c-gitignore-toggle} `zg` hides / shows git-ignored entries.
  Orthogonal to the existing `.` hidden-files toggle. Reuses the
  `project.git_store()` lookup that the git-decorations clause
  already consults.
- {#c-preview-ratio} `<` shrinks and `>` grows the preview column
  width by a fixed step (e.g. 10% of total). Floor at 10%, ceiling
  at 80%.
:::
