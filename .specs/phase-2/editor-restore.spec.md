---
id: TASK:phase-2/editor-restore
type: task
status: accepted
version: 0.1.0
summary: >
  Zed's SerializableItem already covers open files + cursor + scroll + dirty contents.
owners: [carlo]
progress: done
refines:
  - REQ:codon/persistence#c-editor-restore
---

# Editor state restore

Survives codon-session window swaps because item ids are preserved across deserialize.
