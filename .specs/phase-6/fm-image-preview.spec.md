---
id: TASK:phase-6/fm-image-preview
type: task
status: accepted
version: 0.0.1
summary: >
  Render image files in the preview column via Zed's image_viewer
  primitives. Fall back to a metadata placeholder on decode failure.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-preview-richer#c-image-preview
---

# File-manager image preview

## What ships

For files whose extension is in
`["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"]`, the
preview column shows the image scaled to fit. Reuse
[`vendor/zed/crates/image_viewer/`](spec:src:vendor/zed/crates/image_viewer/)
which already produces an `ImageView` element; the FM doesn't need
to depend on the full `Item` registration, just the rendering
primitive.

On decode failure (corrupt file, unsupported variant), fall back
to a placeholder showing: filename, dimensions (if readable from
header alone), file size, mime guess.

## Approach

1. Extend the `Preview` enum in
   [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
   with `Image { path: PathBuf, dimensions: Option<(u32, u32)> }`.
2. The preview-content fetch (currently `read_to_string` for text)
   branches on extension; for images, just record the path —
   actual decode happens on render via image_viewer.
3. View renderer matches on `Preview::Image` and instantiates an
   image element pointing at the path.

~120 LOC. Depends on understanding image_viewer's element API; if
that turns out to require an `Entity<ImageItem>` (heavy state),
fall back to a direct `image::open` + `Img::from_data` for the
preview pane and document the divergence.
