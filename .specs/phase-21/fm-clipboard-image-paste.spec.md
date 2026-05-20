---
id: TASK:phase-21/fm-clipboard-image-paste
type: task
status: draft
version: 0.1.0
summary: >
  When `p` / `P` falls back to the OS clipboard and finds a
  `ClipboardEntry::Image`, save it as a timestamped file in the
  current directory (e.g. `paste-2026-05-20-153012.png`) with an
  extension picked from the `ImageFormat`. Covers the Firefox /
  Slack "right-click → copy image → paste into FM" case.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-os-clipboard#c-fm-paste-from-os
blocked_by:
  - TASK:phase-21/fm-clipboard-paste-fallback
---

# FM image paste — save to current directory

## Plan

The paste-fallback task introduces the `Image` arm in the
`cx.read_from_clipboard()` match. This task fills in the body.

### Behaviour

- File name: `paste-YYYY-MM-DD-HHMMSS.<ext>` in the FM's current
  directory. Use `chrono::Local::now()` for the timestamp (already
  a workspace dependency). If a collision exists, append `-1`,
  `-2`, … to the stem.
- Extension is derived from `Image::format`
  ([`vendor/zed/crates/gpui/src/platform.rs`](spec:src:vendor/zed/crates/gpui/src/platform.rs)
  — `ImageFormat`). Mapping:
  - `Png` → `.png`
  - `Jpeg` → `.jpg`
  - `Gif` → `.gif`
  - `Bmp` → `.bmp`
  - `Tiff` → `.tiff`
  - `Webp` → `.webp`
  - `Svg` → `.svg`
  - anything else → `.bin` and surface a "saved as binary" toast.
- The write goes through `fs.write(path, bytes, …)` so it shares
  error handling with the rest of the FM. Bulk-task pipeline is
  not engaged (single-file op); a one-shot toast on success
  suffices ("Saved paste-2026-05-20-153012.png").

### Why timestamp, not "paste.png"

A single fixed name silently overwrites the previous paste — the
user can't tell whether they're looking at the file they just
pasted or the one from ten minutes ago. The timestamp shape makes
the chronology obvious without needing the user to think about
naming at paste time. The collision-suffix handles the
sub-second case.

### Test path

A GPUI test that builds a synthetic `ClipboardItem` with an
`Image { format: Png, bytes: <16 PNG bytes>, id: 0 }`, calls
`cx.write_to_clipboard(item)`, then drives the paste flow and
asserts a `paste-*.png` file appears in the FM's temp dir with
matching bytes.

## Acceptance

- Right-click → "Copy Image" in Firefox on a PNG, focus codon's
  FM, press `p` → a `paste-*.png` file appears in the current
  directory containing the image bytes.
- Same flow with a JPEG produces `paste-*.jpg`.
- A second paste within the same second produces `-1` etc., no
  overwrites.
- Existing `ExternalPaths` and "empty clipboard" branches in
  `execute_paste` are untouched.
- `spec lint` clean.

## Files touched

- `crates/file-manager/src/file_manager.rs` — flesh out the
  `Image` arm of the OS-fallback match introduced by the
  paste-fallback task.

## Sequencing

Blocked by [TASK:phase-21/fm-clipboard-paste-fallback](spec:TASK:phase-21/fm-clipboard-paste-fallback)
because the `Image` arm is added there as a placeholder toast;
this task replaces the placeholder with the real implementation.
