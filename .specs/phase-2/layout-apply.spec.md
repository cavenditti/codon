---
id: TASK:phase-2/layout-apply
type: task
status: accepted
version: 0.1.0
summary: >
  Workspace::replace_center_with_snapshot drops old panes after building the new tree, preserving item ids.
owners: [carlo]
progress: done
refines:
  - REQ:codon/layout#c-apply
---

# Apply layout snapshot

Added in `vendor/zed/crates/workspace/src/workspace.rs` near serialize_workspace_internal. Returns Task<Result<()>> so callers can detach with notify_err.
