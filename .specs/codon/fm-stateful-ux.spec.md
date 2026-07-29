---
id: REQ:codon/fm-stateful-ux
type: requirement
status: draft
version: 0.1.0
level: SHOULD
summary: >
  Make file-manager loading, errors, preview freshness, selection
  continuity, prefetch, and undo state explicit and keyboard-operable.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-24]
---

# Stateful file-manager UX

:::{requirement id="fm-stateful-ux" level="SHOULD"}
The system SHOULD:

- {#c-explicit-load-state} model a listing as Loading, Ready, or Error.
  During navigation, retained stale rows MUST be visibly marked and
  destructive/list-dependent actions MUST not target them. Permission
  and I/O failures MUST not render as an empty directory.
- {#c-preview-pending-prefetch} label preview content with its source
  path, dim or cover stale content while a new target is pending, and
  prefetch at low priority for at most the nearest adjacent entries.
  Prefetch MUST yield to explicit navigation and respect byte/I/O caps.
- {#c-selection-path-continuity} anchor selection by canonical path
  across reload, sort, filtering, enrichment, and watcher deltas,
  falling back to the nearest surviving neighbor when the path
  disappears.
- {#c-operation-undo} record reversible rename, move, trash, and paste
  operations in task history and expose a keyboard-operable Undo action
  in the completion notification. Conflicts MUST require confirmation
  and failed/partial undo MUST surface a detailed result.
:::
