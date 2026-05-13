---
id: TASK:phase-6/fm-binary-fallback-info
type: task
status: accepted
version: 0.0.1
summary: >
  Replace `[binary]` with a useful preview — size, mime guess, first
  256 bytes as hex + ASCII.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-preview-richer#c-binary-fallback-info
---

# File-manager informative binary fallback

## What ships

When the preview can't render text / image / archive, replace the
bare `[binary]` label with:

```
foo.bin · 4.2 MB · application/octet-stream

00000000  7f 45 4c 46 02 01 01 00  00 00 00 00 00 00 00 00  |.ELF............|
00000010  03 00 3e 00 01 00 00 00  …                       |..>.............|
```

Three lines of header (name · size · mime), then up to 16 lines of
hex (16 bytes per line — first 256 bytes total). Mime guess is
extension-based (the `mime_guess` crate is already in Zed's graph;
prefer over a fresh add).

## Where it slots in

[`crates/file-manager/src/view.rs`](spec:src:crates/file-manager/src/view.rs)
preview branch — current `[binary]` arm. ~120 LOC including the
hex/ASCII formatter (small helper, no external dep needed).
