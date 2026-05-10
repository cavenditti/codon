---
id: REQ:codon/file-manager
type: requirement
status: accepted
version: 1.0.0
level: MUST
summary: >
  Yazi-style three-column file manager with marks, virtualized listing,
  basic file operations, and a directory / file preview pane.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-1]
---

# File manager

:::{requirement id="file-manager" level="MUST"}
The system MUST provide:

- {#c-three-column} parent | current | preview three-column yazi layout
- {#c-navigation} `j`/`k` movement, `h`/`l`/`Enter` traversal, `gg`/`G`
- {#c-scrolling} virtualized rendering via uniform_list, half-page
  Ctrl-d/u, full-page PgUp/PgDn, mouse wheel
- {#c-marks} multi-select via `v`, full-line highlight for marks
- {#c-file-ops} create file (`a`), mkdir (`A`), delete (`d`),
  rename (`r`), yank path (`y`), toggle hidden (`.`)
- {#c-preview} preview pane shows directory listing or first 80 lines
  of a file
:::
