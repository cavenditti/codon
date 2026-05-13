---
id: REQ:codon/fm-trash
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  Trash recovery — list the OS trash, restore to original location,
  permanently delete. Built on the `trash` crate codon already uses
  for `D` (delete-to-trash).
owners: [carlo]
categorized_under: [TOPIC:topics/phase-8]
---

# File manager trash

Phase 5 `D` (shift-d) sends entries to the OS trash via the `trash`
crate. Recovery and permanent-deletion currently require leaving
codon. This requirement adds an in-FM trash browser.

:::{requirement id="fm-trash" level="MAY"}
The file manager SHOULD support:

- {#c-trash-list} `T` (shift-t) opens a modal listing the OS
  trash via `trash::os_limited::list()` — original path, trashed
  timestamp, size. Fuzzy filter at the top filters by original
  path. Reuses the cheatsheet's `gpui::list` virtualization
  pattern so large trash bins (1000+ entries) stay snappy.
- {#c-trash-restore} `Enter` restores the highlighted entry to
  its original location via `trash::os_limited::restore_all`.
  `Space` marks for bulk restore; subsequent `Enter` restores
  every marked entry. Conflicts at the target path surface the
  same numbered-suffix prompt as paste.
- {#c-trash-purge} `X` (shift-x) permanently deletes the
  highlighted / marked entries from the trash via
  `trash::os_limited::purge_all`. Single-prompt confirmation,
  matching `D`'s "delete N entries? y/N" flow. This is also the
  surface for "skip trash, delete now" — `X` on a live file
  prompts then bypasses the trash.
:::
