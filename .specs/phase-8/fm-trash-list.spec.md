---
id: TASK:phase-8/fm-trash-list
type: task
status: accepted
version: 0.0.1
summary: >
  `T` opens a modal listing the OS trash via `trash::os_limited::list`.
  Type-as-you-go fuzzy filter by original path.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-trash#c-trash-list
---

# File-manager trash listing modal

## What ships

`T` (shift-t) opens a workspace-modal listing every entry in the
OS trash, with columns:

- Original path (fuzzy-filtered by an input bar at top)
- Trashed timestamp (relative)
- Size (human units)

Backed by `trash::os_limited::list()` — codon already pulls in
the `trash` crate for phase-5 `D` delete-to-trash, so no new dep.

Modal uses `gpui::list` for virtualization so a 1000+ entry trash
stays responsive.

## Where it slots in

- New module `crates/file-manager/src/trash.rs` — defines
  `TrashListModal` + `TrashEntry` row type.
- Action `codon_fm::TrashList` registered for-workspace.
- ~250 LOC.
