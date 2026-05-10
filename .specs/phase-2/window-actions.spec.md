---
id: TASK:phase-2/window-actions
type: task
status: accepted
version: 0.1.0
summary: >
  WindowNew/Next/Prev/Close + parameterized WindowGoto(usize).
owners: [carlo]
progress: done
refines:
  - REQ:codon/windows#c-actions
---

# Window switching actions

WindowGoto is a Derive(Action) struct so the keymap can pass an index. Other window actions are unit structs declared via `actions!`.
