---
id: TASK:phase-10/fm-syntax-preview
type: task
status: accepted
version: 0.0.1
summary: >
  Replace the plain-text file-manager preview with a read-only
  `editor::Editor` so previewed source files render with the same
  syntax highlighting they would in a normal editor pane. Enrich the
  binary fallback with a friendly type label derived from mime.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/fm-preview-richer#c-syntax-highlight
  - REQ:codon/fm-preview-richer#c-metadata-fallback
aspects: [fm-preview-syntax, fm-preview-metadata-label]
---

# Syntax-highlighted text preview + enriched metadata fallback

## What ships

`crates/file-manager/src/file_manager.rs` learns a new preview
variant:

```rust
pub(crate) enum Preview {
    Directory(Vec<DirEntry>),
    Text(TextPreview),       // replaces FileContent(String)
    Archive(ArchiveListing),
    Image(ImageInfo),
    Binary(BinaryInfo),
    Empty,
}

pub(crate) struct TextPreview {
    pub(crate) path: PathBuf,
    pub(crate) content: String,
    pub(crate) byte_size: u64,
}
```

`Preview::Text` carries the bytes only — the heavy `editor::Editor`
entity is constructed lazily by the view layer when it renders the
preview column, and cached on the `FileManager` keyed by path:

```rust
pub(crate) struct PreviewEditorCache {
    pub(crate) path: PathBuf,
    pub(crate) editor: Entity<editor::Editor>,
}

// new field on FileManager:
pub(crate) preview_editor: Option<PreviewEditorCache>,
pub(crate) language_registry: Option<Arc<LanguageRegistry>>,
```

`language_registry` is resolved once at construction time from
`workspace.read(cx).app_state().languages.clone()` and stored as
`Arc<LanguageRegistry>` so the preview builder never has to climb
back up to the workspace.

When `render_preview` (now `&mut self`) sees `Preview::Text`:

1. If `preview_editor.path == text.path`, render the cached editor.
2. Otherwise build a fresh `Buffer::local(content, cx)` with the
   registry installed via `buffer.set_language_registry(...)`, kick
   off `registry.load_language_for_file_path(&path)` and apply the
   resolved `Language` to the buffer when the task completes
   (highlighting then redraws via the usual buffer observer chain).
   Wrap the buffer in `Editor::for_buffer(buffer, None, window, cx)`,
   call `editor.set_read_only(true)` and stash it in
   `preview_editor`.
3. Render the editor with the same flex/overflow box the old
   `FileContent` branch used.

`update_preview_sync` continues to populate `Preview::Text` with the
file's bytes (up to a generous cap — full file rather than 80
lines, since the editor handles long-buffer scrolling far better
than a `Label`). The 80-line truncation belonged to the old
plain-text rendering; with an editor we keep the full text but the
preview pane only paints what's visible.

For non-text types, `render_binary_preview` gains a single line of
type-aware copy (`render_binary_preview`):

```rust
let type_label = mime_type_label(&info.mime);
// e.g. "Audio file (FLAC)" / "Video file (Matroska)" / "PDF document"
//      / "Font file (TrueType)" / "Binary data"
```

Implemented as a small `match` on the leading mime category
(`audio/*`, `video/*`, `application/pdf`, `font/*`,
`application/x-*` archive subtypes already handled elsewhere) plus
a friendly suffix from the subtype.

## Out of scope

- Decoding audio/video metadata (duration, codec, sample rate).
  Pulling `mediainfo` or `symphonia` in just for the preview pane
  is more weight than the feature warrants; mime-derived labels
  are enough to feel ranger-like.
- Rendering PDF pages. Same reasoning.
- Soft-wrap or word-wrap toggles for the preview editor —
  inherits whatever the user's language settings already say.

## Verification

- Navigate to a `.rs` file: preview column shows Rust syntax
  highlighting (keywords, strings, comments all coloured).
- Navigate to a `.json` file: keys / values / strings highlighted.
- Navigate to a file with no registered language (e.g. `.foo`):
  preview shows plain text, no panic.
- Rapidly hold `j` through a directory of 200 source files:
  preview keeps up (cached editor per path; allocations bounded
  by visit history, not by keystroke count).
- Navigate to an `.mp3`: binary fallback header reads
  `Audio file (MPEG)` (or similar) followed by the hex dump.

## Where it slots in

- Edit: `crates/file-manager/src/file_manager.rs` — `Preview`
  enum, `FileManager` fields, `update_preview_sync` text branch.
- Edit: `crates/file-manager/src/view.rs` — `render_preview`
  signature + new `Preview::Text` arm; `render_binary_preview`
  prepends a type label.
- No vendored-Zed changes; `Editor::for_buffer`, `set_read_only`,
  and `LanguageRegistry::load_language_for_file_path` are all
  already public.
