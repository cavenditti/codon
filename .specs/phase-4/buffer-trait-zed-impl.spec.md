---
id: TASK:phase-4/buffer-trait-zed-impl
type: task
status: accepted
version: 0.2.0
summary: >
  Wontdo 2026-05-13 — the `impl Buffer for language::Buffer`
  shipped originally (46 LOC of forwarding inside the codon-buffer
  crate) but was removed alongside the crate when
  REQ:codon/buffer-trait was superseded. See
  TASK:phase-4/buffer-trait-skeleton for the full reasoning. The
  impl was never invoked — no callsite ever took
  `&dyn codon_buffer::Buffer`.
owners: [carlo]
progress: wontdo
refines:
  - REQ:codon/buffer-trait#c-zed-impl
---

# Buffer impl for language::Buffer (wontdo)

## Original framing (retained for history)

`impl codon_buffer::Buffer for language::Buffer` — every trait method
maps 1:1 to an existing inherent method on `language::Buffer`.

Lives in `crates/codon-buffer/src/zed_impl.rs` to keep the trait
crate free of `language` as a hard dep — gate it behind a `zed-buffer`
feature on `codon-buffer` (default-on) so a future helix-only build can
opt out.

## File anchors

- Source for delegation reference:
  [`vendor/zed/crates/language/src/buffer.rs`](spec:src:vendor/zed/crates/language/src/buffer.rs)
- Crate to extend: [`crates/codon-buffer/`](spec:src:crates/codon-buffer/)

No new types — only the impl block plus the feature gate. The Zed
buffer keeps working unchanged; consumers can take either `&Buffer`
(concrete) or `&dyn codon_buffer::Buffer` (trait object) as needed.
