---
id: TASK:phase-2/native-dialog-audit
type: task
status: accepted
version: 0.1.0
summary: >
  Audited 5 callsites of cx.prompt_for_paths in vendored Zed; none reachable from codon-only flows.
owners: [carlo]
progress: done
refines:
  - REQ:codon/pane-ux#c-no-native-dialogs
---

# Native dialog audit

Replacement of the 5 callsites is tracked under [REQ:codon/in-app-pickers](spec:REQ:codon/in-app-pickers).
