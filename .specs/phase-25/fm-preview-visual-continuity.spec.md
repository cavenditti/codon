---
id: TASK:phase-25/fm-preview-visual-continuity
type: task
status: accepted
version: 0.1.0
summary: >
  Keep the file-manager text-preview surface and typography visually stable
  while its lightweight snapshot upgrades to a full editor.
owners: [carlo]
progress: done
refines: ["REQ:codon/file-manager#c-preview"]
assignee:
eta:
blocked_by: []
---

# FM preview visual continuity

## Plan

- Paint text previews on `editor_background` from the first frame so the
  padded full editor does not appear as a differently colored inset card.
  Keep directory and metadata previews on their existing panel surface.
- Render the deferred static snapshot with the active buffer font, buffer
  font size, buffer line height, editor foreground, and the same content
  insets as the upgraded editor.
- Keep the static snapshot lightweight and clipped to a screenful; syntax
  highlighting and full editor behavior remain deferred until dwell.

## Acceptance

- The static-to-editor handoff changes content fidelity only: background,
  foreground, font family, font size, line height, and outer insets remain
  stable across the upgrade.
- No panel-colored frame is visible around the full text-preview editor.
- Directory previews retain the panel background and existing column styling.
