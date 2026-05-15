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
- {#c-ui} *(superseded)* — original umbrella for ranger-style chrome
  (unified background, rich info bar, contextual help). The work
  landed and was refined into the dedicated REQs
  [REQ:codon/fm-chrome](spec:REQ:codon/fm-chrome),
  [REQ:codon/file-manager-theme](spec:REQ:codon/file-manager-theme),
  and [REQ:codon/fm-enhancements](spec:REQ:codon/fm-enhancements);
  this clause remains as an anchor for the historical commit 423f2f9
  Spec-Ref trailer.
- {#c-esc-semantics} *(superseded)* — original anchor for
  unconditional-Esc handling in the file-manager (drop pending input,
  clear chord, commit visual anchor, restore find origin). The
  behaviour shipped as part of the file-manager state-transition
  logic and is exercised by the `handle_cancel` path; this clause
  remains as an anchor for the historical commit 7a8660b Spec-Ref
  trailer.
:::
