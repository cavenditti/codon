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
- {#c-syntax-highlight} for text files, render content through a
  read-only `editor::Editor` so syntax highlighting matches what the
  user sees when the file is opened. Language is resolved from the
  workspace `LanguageRegistry` by file path (extension + filename
  rules), loaded async so the preview doesn't block on first hit.
  Files without a registered language render as plain (unhighlighted)
  text. The editor entity is cached keyed by path so rapid `j`/`k`
  scrolling reuses the same view instead of allocating per keystroke.
- {#c-metadata-fallback} the binary fallback SHOULD surface a
  human-readable type label derived from the mime guess (Audio,
  Video, PDF, Font, …) above the hex dump, so non-renderable types
  feel informative rather than opaque. This is metadata-only — no
  decoding (audio duration, video codec, PDF page count) is required.
:::
