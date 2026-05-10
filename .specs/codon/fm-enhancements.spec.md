---
id: REQ:codon/fm-enhancements
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  File manager polish — fuzzy filter, git status indicators, copy/paste
  files, bulk operations on marked files.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-5]
---

# File manager enhancements

:::{requirement id="fm-enhancements" level="MAY"}
The system SHOULD provide:

- {#c-fuzzy-filter} `/` enters Insert mode for filtering the current
  directory listing
- {#c-git-indicators} per-entry git status decorations (M / A / D / ?)
- {#c-copy-paste} `y` / `d` modal operations for file copy / move
- {#c-bulk-ops} bulk rename / delete / copy on marked files
:::
