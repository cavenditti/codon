---
id: TASK:phase-4/buffer-trait-zed-impl
type: task
status: accepted
version: 0.0.1
summary: >
  Implement codon_buffer::Buffer for language::Buffer — pure delegation.
owners: [carlo]
progress: done
refines:
  - REQ:codon/buffer-trait#c-zed-impl
---

# Buffer impl for language::Buffer

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
