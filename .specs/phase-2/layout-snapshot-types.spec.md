---
id: TASK:phase-2/layout-snapshot-types
type: task
status: accepted
version: 0.1.0
summary: >
  Serde-friendly LayoutSnapshot mirroring Zed's SerializedPaneGroup, with a Stack variant.
owners: [carlo]
progress: done
refines:
  - REQ:codon/layout#c-snapshot-types
---

# LayoutSnapshot types

Lives in `vendor/zed/crates/workspace/src/codon_bridge.rs`. Public to consumers; mirrors the internal SerializedPaneGroup tree.
