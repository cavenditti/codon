---
id: REQ:codon/fm-preview-richer
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  Preview-pane content beyond plain text — image, archive listing,
  informative binary fallback.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-6]
---

# Richer file-manager preview

Today the preview column renders the first 80 lines of a text file,
the dir's children if the selection is a directory, or `[binary]`.
That last branch covers everything codon doesn't know how to render —
images, archives, audio, video — which is most non-source files. This
requirement teaches the preview about image and archive content and
turns the binary fallback into something the user can actually read.

:::{requirement id="fm-preview-richer" level="MAY"}
The file manager preview pane SHOULD support:

- {#c-image-preview} for image extensions (`.png` / `.jpg` /
  `.jpeg` / `.gif` / `.webp` / `.bmp` / `.ico`), render the image
  in the preview column. Reuse Zed's `image_viewer::ImageItem` /
  `ImageView` (already wired for full-pane open from
  TASK:phase-5/image-preview-pane). Fall back to a placeholder
  showing dimensions + size if decode fails.
- {#c-archive-preview} for archive extensions (`.zip` / `.tar` /
  `.tar.gz` / `.tgz` / `.7z`), list the archive's top-level entries
  (no extraction). Crate dependencies kept minimal: prefer the
  archive crates Zed already pulls in, else add `zip` / `tar`
  directly.
- {#c-binary-fallback-info} for everything else (binary), show:
  file size in human units, mime guess (from extension), and the
  first 256 bytes rendered as hex + ASCII side by side. Replaces the
  bare `[binary]` label.
:::
