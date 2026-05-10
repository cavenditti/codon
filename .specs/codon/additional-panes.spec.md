---
id: REQ:codon/additional-panes
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  Diff viewer, image preview, diagnostics — three pane types that
  complete codon's daily-driver surface.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-5]
---

# Additional panes

:::{requirement id="additional-panes" level="MAY"}
The system SHOULD provide:

- {#c-diff-viewer} a diff viewer pane (likely thin wrapper over the
  git pane's diff component)
- {#c-image-preview} an image preview pane
- {#c-diagnostics} a diagnostics pane with j/k navigation
:::
