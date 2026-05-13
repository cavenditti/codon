---
id: TASK:phase-5/image-preview-pane
type: task
status: accepted
version: 0.0.1
summary: >
  Register Zed's existing image_viewer::ImageView as a codon pane —
  default keymap entry, file-manager hand-off for image extensions.
owners: [carlo]
progress: done
refines:
  - REQ:codon/additional-panes#c-image-preview
---

# Image preview pane

## What ships

Image files (`.png`, `.jpg`, `.gif`, `.webp`, etc.) open in
[`vendor/zed/crates/image_viewer/`](spec:src:vendor/zed/crates/image_viewer/)
when opened from the file manager or command palette. The pane is
already a fully-implemented `workspace::Item` upstream.

## Approach

Almost entirely wiring:

1. Confirm `image_viewer::init(cx)` runs in `apps/codon/src/main.rs`
   (it probably already does — verify and add if missing).
2. File manager: when `Enter` is pressed on a file with an image
   extension, dispatch the same path the project picker uses.
   `crates/file-manager/src/file_manager.rs::handle_open` is where the
   branching goes.
3. Default keymap: no new global binding needed — opens via Enter on
   a file like any other type.

Net: < 30 LOC. The bulk is verifying the existing `image_viewer`
crate works inside codon's reduced surface.
